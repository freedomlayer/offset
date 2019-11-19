use std::collections::{HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::{PhantomData, Unpin};
use std::mem;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, select, stream, FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxFuture, BoxStream, ConnPairVec, FutTransform};
use common::select_streams::select_streams;
use timer::TimerClient;

use proto::crypto::PublicKey;

#[derive(Debug)]
pub struct ConnectPoolClientError;

#[derive(Debug)]
pub struct CpConnectRequest {
    pub response_sender: oneshot::Sender<ConnPairVec>,
}

#[derive(Clone)]
pub struct CpConnectClient {
    request_sender: mpsc::Sender<CpConnectRequest>,
}

pub struct CpConfigClient<RA> {
    request_sender: mpsc::Sender<Vec<RA>>,
}

impl<RA> CpConfigClient<RA> {
    pub fn new(request_sender: mpsc::Sender<Vec<RA>>) -> Self {
        CpConfigClient { request_sender }
    }

    pub async fn config(&mut self, config: Vec<RA>) -> Result<(), ConnectPoolClientError> {
        self.request_sender
            .send(config)
            .await
            .map_err(|_| ConnectPoolClientError)?;
        Ok(())
    }
}

impl CpConnectClient {
    pub fn new(request_sender: mpsc::Sender<CpConnectRequest>) -> Self {
        CpConnectClient { request_sender }
    }

    pub async fn connect(&mut self) -> Result<ConnPairVec, ConnectPoolClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let connect_request = CpConnectRequest { response_sender };
        self.request_sender
            .send(connect_request)
            .await
            .map_err(|_| ConnectPoolClientError)?;

        response_receiver.await.map_err(|_| ConnectPoolClientError)
    }
}

#[derive(Debug)]
pub enum ConnectPoolError {
    SpawnError,
    MultipleConnectRequests,
}

#[derive(Debug)]
enum CpEvent<RA> {
    ConnectRequest(CpConnectRequest),
    ConnectRequestClosed,
    ConfigRequest(Vec<RA>),
    ConfigRequestClosed,
    ConnectAttemptDone(Option<ConnPairVec>),
    TimerTick,
    TimerClosed,
}

enum CpStatus<RA> {
    NoRequest,
    Waiting((usize, oneshot::Sender<ConnPairVec>)),
    Connecting((RA, oneshot::Sender<()>, oneshot::Sender<ConnPairVec>)),
}

struct ConnectPool<RA, C, ET, S> {
    friend_public_key: PublicKey,
    addresses: VecDeque<RA>,
    status: CpStatus<RA>,
    conn_done_sender: mpsc::Sender<Option<ConnPairVec>>,
    backoff_ticks: usize,
    client_connector: C,
    encrypt_transform: ET,
    spawner: S,
}

async fn conn_attempt<RA, C, ET>(
    friend_public_key: PublicKey,
    address: RA,
    mut client_connector: C,
    mut encrypt_transform: ET,
    canceler: oneshot::Receiver<()>,
) -> Option<ConnPairVec>
where
    RA: Eq,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>> + Clone,
{
    // TODO: How to remove this Box::pin?
    let connect_fut = Box::pin(async move {
        let raw_conn = client_connector
            .transform((address, friend_public_key.clone()))
            .await?;
        encrypt_transform
            .transform((friend_public_key.clone(), raw_conn))
            .await
    });

    // We either finish connecting, or got canceled in the middle:
    select! {
        connect_fut = connect_fut.fuse() => connect_fut,
        _ = canceler.fuse() => None,
    }
}

impl<RA, C, ET, S> ConnectPool<RA, C, ET, S>
where
    RA: Hash + Clone + Eq + Send + Debug + 'static,
    S: Spawn,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>>
        + Clone
        + Send
        + 'static,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone + Send + 'static,
{
    pub fn new(
        friend_public_key: PublicKey,
        conn_done_sender: mpsc::Sender<Option<ConnPairVec>>,
        backoff_ticks: usize,
        client_connector: C,
        encrypt_transform: ET,
        spawner: S,
    ) -> Self {
        ConnectPool {
            friend_public_key,
            addresses: VecDeque::new(),
            status: CpStatus::NoRequest,
            conn_done_sender,
            backoff_ticks,
            client_connector,
            encrypt_transform,
            spawner,
        }
    }

    /// Start a connection attempt through a relay with a given address.
    /// Returns a canceler.
    fn create_conn_attempt(
        &mut self,
        address: RA,
    ) -> Result<oneshot::Sender<()>, ConnectPoolError> {
        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let c_friend_public_key = self.friend_public_key.clone();
        let c_client_connector = self.client_connector.clone();
        let c_encrypt_transform = self.encrypt_transform.clone();

        let mut c_conn_done_sender = self.conn_done_sender.clone();
        let conn_fut = async move {
            let opt_conn = conn_attempt(
                c_friend_public_key,
                address,
                c_client_connector,
                c_encrypt_transform,
                cancel_receiver,
            )
            .await;
            let _ = c_conn_done_sender.send(opt_conn).await;
        };

        self.spawner
            .spawn(conn_fut)
            .map_err(|_| ConnectPoolError::SpawnError)?;

        Ok(cancel_sender)
    }

    pub fn handle_connect_request(
        &mut self,
        connect_request: CpConnectRequest,
    ) -> Result<(), ConnectPoolError> {
        if let CpStatus::NoRequest = self.status {
        } else {
            return Err(ConnectPoolError::MultipleConnectRequests);
        }

        let address = match self.addresses.pop_front() {
            None => {
                // We can't connect yet, because we don't know of any address.
                self.status = CpStatus::Waiting((0, connect_request.response_sender));
                return Ok(());
            }
            Some(address) => address,
        };

        let canceler = self.create_conn_attempt(address.clone())?;
        self.status = CpStatus::Connecting((address, canceler, connect_request.response_sender));
        Ok(())
    }

    fn add_address(&mut self, address: RA) -> Result<(), ConnectPoolError> {
        let was_empty = self.addresses.is_empty();
        if !self.addresses.contains(&address) {
            self.addresses.push_back(address);
        }

        let status = mem::replace(&mut self.status, CpStatus::NoRequest);
        match (was_empty, status) {
            (true, CpStatus::Waiting((_remaining_ticks, response_sender))) => {
                let address = self.addresses.pop_front().unwrap();
                let canceler = self.create_conn_attempt(address.clone())?;
                self.status = CpStatus::Connecting((address, canceler, response_sender));
            }
            (_, status) => self.status = status,
        };
        Ok(())
    }

    fn remove_address(&mut self, address: RA) -> Result<(), ConnectPoolError> {
        self.addresses.retain(|cur_address| cur_address != &address);
        match mem::replace(&mut self.status, CpStatus::NoRequest) {
            CpStatus::NoRequest => {}
            CpStatus::Waiting(waiting) => {
                self.status = CpStatus::Waiting(waiting);
            }
            CpStatus::Connecting((cur_address, canceler, response_sender)) => {
                if address == cur_address {
                    // We were trying to connect to the address being removed:
                    let _ = canceler.send(());
                    if let Some(address) = self.addresses.pop_front() {
                        // There is another address we can use:
                        let canceler = self.create_conn_attempt(address.clone())?;
                        self.status = CpStatus::Connecting((address, canceler, response_sender));
                    } else {
                        // There is no other address:
                        self.status = CpStatus::Waiting((0, response_sender));
                    }
                } else {
                    self.status = CpStatus::Connecting((cur_address, canceler, response_sender));
                }
            }
        };
        Ok(())
    }

    pub fn handle_config_request(&mut self, config: Vec<RA>) -> Result<(), ConnectPoolError> {
        let old_addresses = self.addresses.iter().cloned().collect::<HashSet<_>>();

        let new_addresses: HashSet<RA> = config.into_iter().collect::<HashSet<_>>();

        for removed_address in old_addresses.difference(&new_addresses) {
            self.remove_address(removed_address.clone())?;
        }

        for added_address in new_addresses.difference(&old_addresses) {
            self.add_address(added_address.clone())?;
        }
        Ok(())
    }

    pub fn handle_timer_tick(&mut self) -> Result<(), ConnectPoolError> {
        let waiting = match mem::replace(&mut self.status, CpStatus::NoRequest) {
            CpStatus::Waiting(waiting) => waiting,
            other_status => {
                self.status = other_status;
                return Ok(());
            }
        };

        let (mut backoff_ticks, response_sender) = waiting;
        backoff_ticks = backoff_ticks.saturating_sub(1);
        if backoff_ticks == 0 {
            if let Some(address) = self.addresses.pop_front() {
                let canceler = self.create_conn_attempt(address.clone())?;
                self.status = CpStatus::Connecting((address, canceler, response_sender));
            } else {
                self.status = CpStatus::Waiting((self.backoff_ticks, response_sender));
            }
        } else {
            self.status = CpStatus::Waiting((backoff_ticks, response_sender));
        }
        Ok(())
    }

    pub fn handle_connect_attempt_done(&mut self, opt_conn: Option<ConnPairVec>) {
        let connecting = match mem::replace(&mut self.status, CpStatus::NoRequest) {
            CpStatus::NoRequest | CpStatus::Waiting(_) => unreachable!(),
            CpStatus::Connecting(connecting) => connecting,
        };

        let (address, _canceler, response_sender) = connecting;
        self.addresses.push_back(address);

        if let Some(conn) = opt_conn {
            if let Err(e) = response_sender.send(conn) {
                warn!(
                    "handle_connect_attempt_done(): Failed to send connection response: {:?}",
                    e
                );
            }
            self.status = CpStatus::NoRequest;
        } else {
            self.status = CpStatus::Waiting((self.backoff_ticks, response_sender));
        }
    }
}

async fn connect_pool_loop<RA, ET, TS, C, S>(
    incoming_requests: mpsc::Receiver<CpConnectRequest>,
    incoming_config: mpsc::Receiver<Vec<RA>>,
    timer_stream: TS,
    encrypt_transform: ET,
    friend_public_key: PublicKey,
    backoff_ticks: usize,
    client_connector: C,
    spawner: S,
    mut opt_event_sender: Option<mpsc::Sender<()>>,
) -> Result<(), ConnectPoolError>
where
    RA: Hash + Clone + Eq + Send + Debug + 'static,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    TS: Stream + Unpin + Send,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone,
{
    let (conn_done_sender, incoming_conn_done) = mpsc::channel(0);
    let mut connect_pool = ConnectPool::new(
        friend_public_key,
        conn_done_sender,
        backoff_ticks,
        client_connector,
        encrypt_transform,
        spawner.clone(),
    );

    let incoming_conn_done = incoming_conn_done.map(CpEvent::<RA>::ConnectAttemptDone);

    let incoming_requests = incoming_requests
        .map(CpEvent::<RA>::ConnectRequest)
        .chain(stream::once(future::ready(CpEvent::ConnectRequestClosed)));

    let incoming_config = incoming_config
        .map(CpEvent::ConfigRequest)
        .chain(stream::once(future::ready(CpEvent::ConfigRequestClosed)));

    let incoming_ticks = timer_stream
        .map(|_| CpEvent::TimerTick)
        .chain(stream::once(future::ready(CpEvent::TimerClosed)));

    let mut incoming_events = select_streams![
        incoming_conn_done,
        incoming_requests,
        incoming_config,
        incoming_ticks
    ];

    while let Some(event) = incoming_events.next().await {
        match event {
            CpEvent::ConnectRequest(connect_request) => {
                connect_pool.handle_connect_request(connect_request)?
            }
            CpEvent::ConnectRequestClosed => {
                info!("connect_pool_loop(): connect request closed");
                break;
            }
            CpEvent::ConfigRequest(config) => connect_pool.handle_config_request(config)?,
            CpEvent::ConfigRequestClosed => {
                info!("connect_pool_loop(): config request closed");
                break;
            }
            CpEvent::TimerTick => connect_pool.handle_timer_tick()?,
            CpEvent::TimerClosed => {
                info!("connect_pool_loop(): timer closed");
                break;
            }
            CpEvent::ConnectAttemptDone(opt_conn) => {
                connect_pool.handle_connect_attempt_done(opt_conn)
            }
        }
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    info!("connect_pool_loop() exit");
    Ok(())
}

pub type ConnectPoolControl<RA> = (CpConfigClient<RA>, CpConnectClient);

pub fn create_connect_pool<RA, ET, TS, C, S>(
    timer_stream: TS,
    encrypt_transform: ET,
    friend_public_key: PublicKey,
    backoff_ticks: usize,
    client_connector: C,
    spawner: S,
) -> Result<ConnectPoolControl<RA>, ConnectPoolError>
where
    RA: Hash + Clone + Eq + Send + Debug + 'static,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    TS: Stream + Unpin + Send + 'static,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let (connect_request_sender, incoming_requests) = mpsc::channel(0);
    let (config_request_sender, incoming_config) = mpsc::channel(0);

    let loop_fut = connect_pool_loop(
        incoming_requests,
        incoming_config,
        timer_stream,
        encrypt_transform,
        friend_public_key,
        backoff_ticks,
        client_connector,
        spawner.clone(),
        None,
    )
    .map_err(|e| error!("connect_pool_loop() error: {:?}", e))
    .map(|_| ());

    spawner
        .spawn(loop_fut)
        .map_err(|_| ConnectPoolError::SpawnError)?;

    Ok((
        CpConfigClient::new(config_request_sender),
        CpConnectClient::new(connect_request_sender),
    ))
}

#[derive(Clone)]
pub struct PoolConnector<RA, C, ET, S> {
    timer_client: TimerClient,
    client_connector: C,
    encrypt_transform: ET,
    backoff_ticks: usize,
    spawner: S,
    phantom_b: PhantomData<RA>,
}

impl<RA, C, ET, S> PoolConnector<RA, C, ET, S>
where
    RA: Hash + Clone + Eq + Send + 'static,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    pub fn new(
        timer_client: TimerClient,
        client_connector: C,
        encrypt_transform: ET,
        backoff_ticks: usize,
        spawner: S,
    ) -> Self {
        PoolConnector {
            timer_client,
            client_connector,
            encrypt_transform,
            backoff_ticks,
            spawner,
            phantom_b: PhantomData,
        }
    }
}

impl<RA, C, ET, S> FutTransform for PoolConnector<RA, C, ET, S>
where
    RA: Hash + Clone + Eq + Send + Debug + 'static,
    C: FutTransform<Input = (RA, PublicKey), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    ET: FutTransform<Input = (PublicKey, ConnPairVec), Output = Option<ConnPairVec>>
        + Clone
        + Send
        + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Input = PublicKey;
    type Output = ConnectPoolControl<RA>;

    fn transform(&mut self, friend_public_key: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            // TODO: Should we keep the unwrap()-s here?
            let timer_stream = self.timer_client.request_timer_stream().await.unwrap();
            create_connect_pool(
                timer_stream,
                self.encrypt_transform.clone(),
                friend_public_key,
                self.backoff_ticks,
                self.client_connector.clone(),
                self.spawner.clone(),
            )
            .unwrap()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{block_on, ThreadPool};
    use futures::future::join;

    use common::conn::FuncFutTransform;
    use common::dummy_connector::DummyConnector;

    use timer::{dummy_timer_multi_sender, TimerTick};

    async fn task_pool_connector_cyclic_connect<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, timer_client) = dummy_timer_multi_sender(spawner.clone());

        let backoff_ticks = 2;

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel(0);
        let client_connector = DummyConnector::new(conn_request_sender);

        // We don't need encryption for this test:
        let encrypt_transform = FuncFutTransform::new(|(_opt_public_key, conn_pair)| {
            Box::pin(future::ready(Some(conn_pair)))
        });

        let mut pool_connector = PoolConnector::<u32, _, _, _>::new(
            timer_client,
            client_connector,
            encrypt_transform,
            backoff_ticks,
            spawner,
        );

        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let (mut config_client, mut connect_client) = pool_connector.transform(pk_b.clone()).await;
        let _tick_sender = tick_sender_receiver.next().await.unwrap();

        let addresses = vec![0x0u32, 0x1u32, 0x2u32];
        config_client.config(addresses.clone()).await.unwrap();

        // Addresses that we have seen an attempt to connect to:
        let mut observed_addresses = Vec::new();

        // Connect and handle the connection request at the same time
        let connect_fut = connect_client.connect();
        let handle_connect_fut = async {
            let conn_request = conn_request_receiver.next().await.unwrap();
            let (local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, local_receiver) = mpsc::channel(0);

            let (address, pk) = &conn_request.address;
            observed_addresses.push(address.clone());
            assert_eq!(pk, &pk_b);

            conn_request.reply(Some(ConnPairVec::from_raw(local_sender, local_receiver)));
            (conn_request_receiver, (remote_sender, remote_receiver))
        };
        let (local_conn, (new_conn_request_receiver, _remote_conn)) =
            join(connect_fut, handle_connect_fut).await;
        let mut conn_request_receiver = new_conn_request_receiver;

        // Drop the connection:
        drop(local_conn);

        // Request a new connection:
        let connect_fut = connect_client.connect();
        let handle_connect_fut = async {
            let conn_request = conn_request_receiver.next().await.unwrap();
            let (local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, local_receiver) = mpsc::channel(0);

            let (address, pk) = &conn_request.address;
            observed_addresses.push(address.clone());
            assert_eq!(pk, &pk_b);

            conn_request.reply(Some(ConnPairVec::from_raw(local_sender, local_receiver)));
            (conn_request_receiver, (remote_sender, remote_receiver))
        };
        let (local_conn, (new_conn_request_receiver, _remote_conn)) =
            join(connect_fut, handle_connect_fut).await;
        let mut conn_request_receiver = new_conn_request_receiver;

        // Drop the connection:
        drop(local_conn);

        // Request a new connection:
        let connect_fut = connect_client.connect();
        let handle_connect_fut = async {
            let conn_request = conn_request_receiver.next().await.unwrap();
            let (local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, local_receiver) = mpsc::channel(0);

            let (address, pk) = &conn_request.address;
            observed_addresses.push(address.clone());
            assert_eq!(pk, &pk_b);

            conn_request.reply(Some(ConnPairVec::from_raw(local_sender, local_receiver)));
            (conn_request_receiver, (remote_sender, remote_receiver))
        };
        let (local_conn, (new_conn_request_receiver, _remote_conn)) =
            join(connect_fut, handle_connect_fut).await;
        let mut conn_request_receiver = new_conn_request_receiver;

        // Drop the connection:
        drop(local_conn);

        // There should be exactly 3 observed addresses:
        let unique_observed = observed_addresses.iter().cloned().collect::<HashSet<_>>();
        assert_eq!(unique_observed.len(), 3);

        // Request a new connection:
        let connect_fut = connect_client.connect();
        let handle_connect_fut = async move {
            let conn_request = conn_request_receiver.next().await.unwrap();
            let (local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, local_receiver) = mpsc::channel(0);

            // We expect cyclic attempts.
            // This time the first observed_address should be attempted again:
            let (address, pk) = &conn_request.address;
            assert_eq!(pk, &pk_b);
            assert_eq!(address, &observed_addresses[0]);

            conn_request.reply(Some(ConnPairVec::from_raw(local_sender, local_receiver)));
            (conn_request_receiver, (remote_sender, remote_receiver))
        };
        let (_local_conn, (new_conn_request_receiver, _remote_conn)) =
            join(connect_fut, handle_connect_fut).await;
        let _conn_request_receiver = new_conn_request_receiver;
    }

    #[test]
    fn test_pool_connector_cyclic_connect() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_pool_connector_cyclic_connect(thread_pool.clone()));
    }

    async fn task_pool_connector_backoff_ticks<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, mut timer_client) =
            dummy_timer_multi_sender(spawner.clone());

        let backoff_ticks = 2;

        let (conn_request_sender, mut conn_request_receiver) = mpsc::channel(0);
        let client_connector = DummyConnector::new(conn_request_sender);

        // We don't need encryption for this test:
        let encrypt_transform = FuncFutTransform::new(|(_public_key, conn_pair)| {
            Box::pin(future::ready(Some(conn_pair)))
        });

        let timer_stream = timer_client.request_timer_stream().await.unwrap();
        let mut tick_sender = tick_sender_receiver.next().await.unwrap();

        // Used for debugging the loop:
        let (event_sender, mut event_receiver) = mpsc::channel(0);

        let (request_sender, incoming_requests) = mpsc::channel(0);
        let (config_sender, incoming_config) = mpsc::channel(0);

        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);

        // We call connect_pool_loop directly instead of using the wrapper here.
        // This is done because we need the event_sender if we want precise tests for
        // time ticks.
        //
        // If we don't use event_sender we might be sending timer ticks that are discarded, because
        // they are not received at the correct time.
        let loop_fut = connect_pool_loop(
            incoming_requests,
            incoming_config,
            timer_stream,
            encrypt_transform,
            pk_b.clone(), // friend_public_key
            backoff_ticks,
            client_connector,
            spawner.clone(),
            Some(event_sender),
        )
        .map_err(|e| error!("connect_pool_loop() error: {:?}", e))
        .map(|_| ());

        spawner.spawn(loop_fut).unwrap();

        let mut connect_client = CpConnectClient::new(request_sender);
        let mut config_client = CpConfigClient::new(config_sender);

        let addresses = vec![0x0u32, 0x1u32, 0x2u32];
        config_client.config(addresses.clone()).await.unwrap();
        event_receiver.next().await.unwrap();

        // Addresses that we have seen an attempt to connect to:
        let mut observed_addresses = Vec::new();

        // Connect and handle the connection request at the same time
        let connect_fut = connect_client.connect();
        let handle_connect_fut = async {
            event_receiver.next().await.unwrap(); // Connection request event
            for _ in 0..addresses.len() {
                let conn_request = conn_request_receiver.next().await.unwrap();

                let (address, pk) = &conn_request.address;
                observed_addresses.push(address.clone());
                assert_eq!(pk, &pk_b);

                // Connection attempt failed:
                conn_request.reply(None);
                event_receiver.next().await.unwrap(); // connection attempt done event

                // Wait backoff_ticks:
                for _ in 0..backoff_ticks {
                    tick_sender.send(TimerTick).await.unwrap();
                    event_receiver.next().await.unwrap(); // timer tick event
                }
            }

            // Finally, we let the connection request succeed:
            let conn_request = conn_request_receiver.next().await.unwrap();

            let (local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, local_receiver) = mpsc::channel(0);

            // We expect cyclic attempts.
            // This time the first observed_address should be attempted again:
            let (address, pk) = &conn_request.address;
            assert_eq!(pk, &pk_b);
            assert_eq!(address, &observed_addresses[0]);

            conn_request.reply(Some(ConnPairVec::from_raw(local_sender, local_receiver)));
            event_receiver.next().await.unwrap(); // connection attempt done event
            (conn_request_receiver, (remote_sender, remote_receiver))
        };
        let (local_conn, (_remote_conn, new_conn_request_receiver)) =
            join(connect_fut, handle_connect_fut).await;
        let _conn_request_receiver = new_conn_request_receiver;

        // Drop the connection:
        drop(local_conn);
    }

    #[test]
    fn test_pool_connector_backoff_ticks() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_pool_connector_backoff_ticks(thread_pool.clone()));
    }
}
