use std::mem;
use std::marker::{Unpin, PhantomData};
use std::hash::Hash;
use std::collections::{VecDeque, HashSet};

use futures::channel::{mpsc, oneshot};
use futures::{select, future, FutureExt, TryFutureExt, 
    stream, Stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};

use timer::TimerClient;
use common::conn::{FutTransform, BoxFuture};

use crypto::identity::PublicKey;
use crate::types::RawConn;

#[derive(Debug)]
pub struct ConnectPoolClientError;

pub struct CpConnectRequest {
    response_sender: oneshot::Sender<RawConn>,
}

#[derive(Clone)]
pub struct CpConnectClient {
    request_sender: mpsc::Sender<CpConnectRequest>,
}

pub struct CpConfigClient<B> {
    request_sender: mpsc::Sender<Vec<B>>,
}

impl<B> CpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<Vec<B>>) -> Self {
        CpConfigClient {
            request_sender,
        }
    }

    pub async fn config(&mut self, config: Vec<B>) -> Result<(), ConnectPoolClientError> {
        await!(self.request_sender.send(config))
            .map_err(|_| ConnectPoolClientError)?;
        Ok(())
    }
}

impl CpConnectClient {
    pub fn new(request_sender: mpsc::Sender<CpConnectRequest>) -> Self {
        CpConnectClient {
            request_sender,
        }
    }

    pub async fn connect(&mut self) -> Result<RawConn, ConnectPoolClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let connect_request = CpConnectRequest {
            response_sender,
        };
        await!(self.request_sender.send(connect_request))
            .map_err(|_| ConnectPoolClientError)?;

        await!(response_receiver)
            .map_err(|_| ConnectPoolClientError)
    }
}

#[derive(Debug)]
pub enum ConnectPoolError {
    SpawnError,
    ConnectRequestClosed,
    ConfigRequestClosed,
    TimerClosed,
    MultipleConnectRequests,
}

enum CpEvent<B> {
    ConnectRequest(CpConnectRequest),
    ConnectRequestClosed,
    ConfigRequest(Vec<B>),
    ConfigRequestClosed,
    ConnectAttemptDone(Option<RawConn>),
    TimerTick,
    TimerClosed,
}

enum CpStatus<B> {
    NoRequest,
    Waiting((usize, oneshot::Sender<RawConn>)),
    Connecting((B, oneshot::Sender<()>, oneshot::Sender<RawConn>)),
}

struct ConnectPool<B,C,ET,S> {
    friend_public_key: PublicKey,
    addresses: VecDeque<B>,
    status: CpStatus<B>,
    conn_done_sender: mpsc::Sender<Option<RawConn>>,
    backoff_ticks: usize,
    client_connector: C,
    encrypt_transform: ET,
    spawner: S,
}

async fn conn_attempt<B,C,ET>(friend_public_key: PublicKey, 
                      address: B,
                      mut client_connector: C,
                      mut encrypt_transform: ET,
                      canceler: oneshot::Receiver<()>) -> Option<RawConn>
where
    B: Eq,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone,
{
    // TODO; How to remove this Box::pin?
    let connect_fut = Box::pin(async move {
        let raw_conn = await!(client_connector.transform((address, friend_public_key.clone())))?;
        await!(encrypt_transform.transform((Some(friend_public_key.clone()), raw_conn)))
    });

    // We either finish connecting, or got canceled in the middle:
    select! {
        connect_fut = connect_fut.fuse() => connect_fut,
        _ = canceler.fuse() => None,
    }
}

impl<B,C,ET,S> ConnectPool<B,C,ET,S> 
where
    B: Hash + Clone + Eq + Send + 'static,
    S: Spawn,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
{
    pub fn new(friend_public_key: PublicKey, 
               conn_done_sender: mpsc::Sender<Option<RawConn>>,
               backoff_ticks: usize,
               client_connector: C,
               encrypt_transform: ET,
               spawner: S) -> Self {

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
    fn create_conn_attempt(&mut self, address: B) 
        -> Result<oneshot::Sender<()>, ConnectPoolError> {

        let (cancel_sender, cancel_receiver) = oneshot::channel();
        let c_friend_public_key = self.friend_public_key.clone();
        let c_client_connector = self.client_connector.clone();
        let c_encrypt_transform = self.encrypt_transform.clone();

        let mut c_conn_done_sender = self.conn_done_sender.clone();
        let conn_fut = async move {
            let opt_conn = await!(conn_attempt(c_friend_public_key.clone(), 
                                        address,
                                        c_client_connector.clone(),
                                        c_encrypt_transform.clone(),
                                        cancel_receiver));
            let _ = await!(c_conn_done_sender.send(opt_conn));
        };

        self.spawner.spawn(conn_fut)
            .map_err(|_| ConnectPoolError::SpawnError)?;

        Ok(cancel_sender)
    }

    pub fn handle_connect_request(&mut self, connect_request: CpConnectRequest) 
        -> Result<(), ConnectPoolError> {

        if let CpStatus::NoRequest = self.status {
        } else {
            return Err(ConnectPoolError::MultipleConnectRequests);
        }

        let address = match self.addresses.pop_front() {
            None => {
                self.status = CpStatus::Waiting((0, connect_request.response_sender));
                return Ok(());
            },
            Some(address) => address,
        };

        let canceler = self.create_conn_attempt(address.clone())?;
        self.status = CpStatus::Connecting((address, canceler, connect_request.response_sender));
        Ok(())
    }

    fn add_address(&mut self, address: B) -> Result<(), ConnectPoolError> {
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
            },
            (_, status) => self.status = status,
        };
        Ok(())
    }

    fn remove_address(&mut self, address: B) -> Result<(), ConnectPoolError> {
        self.addresses.retain(|cur_address| cur_address != &address);
        match mem::replace(&mut self.status, CpStatus::NoRequest) {
            CpStatus::NoRequest => {},
            CpStatus::Waiting(waiting) => {
                self.status = CpStatus::Waiting(waiting);
            }, 
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

    pub fn handle_config_request(&mut self, config: Vec<B>) 
        -> Result<(), ConnectPoolError> {

        let old_addresses = self.addresses
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        let new_addresses: HashSet<B> = config
            .into_iter()
            .collect::<HashSet<_>>();

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
                return Ok(())
            },
        };

        let (mut backoff_ticks, response_sender) = waiting;
        backoff_ticks = backoff_ticks.saturating_sub(1);
        if backoff_ticks == 0 {
            if let Some(address) = self.addresses.pop_front() {
                let canceler = self.create_conn_attempt(address.clone())?;
                self.status = CpStatus::Connecting((address, canceler, response_sender));
            }
        } else {
            self.status = CpStatus::Waiting((backoff_ticks, response_sender));
        }
        Ok(())
    }

    pub fn handle_connect_attempt_done(&mut self, opt_conn: Option<RawConn>) {
        let connecting = match mem::replace(&mut self.status, CpStatus::NoRequest) {
            CpStatus::NoRequest | 
            CpStatus::Waiting(_) => unreachable!(),
            CpStatus::Connecting(connecting) => connecting,
        };

        let (address, _canceler, response_sender) = connecting; 

        if let Some(conn) = opt_conn {
            self.addresses.push_back(address);
            if let Err(e) = response_sender.send(conn) {
                warn!("handle_connect_attempt_done(): Failed to send connection response: {:?}", e);
            }
            self.status = CpStatus::NoRequest;
        } else {
            self.status = CpStatus::Waiting((self.backoff_ticks, response_sender));
        }
    }
}


async fn connect_pool_loop<B,ET,TS,C,S>(incoming_requests: mpsc::Receiver<CpConnectRequest>,
                            incoming_config: mpsc::Receiver<Vec<B>>,
                            timer_stream: TS,
                            encrypt_transform: ET,
                            friend_public_key: PublicKey,
                            backoff_ticks: usize,
                            client_connector: C,
                            spawner: S) -> Result<(), ConnectPoolError>
where
    B: Hash + Clone + Eq + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    S: Spawn + Clone,
{

    let (conn_done_sender, incoming_conn_done) = mpsc::channel(0);
    let mut connect_pool = ConnectPool::new(friend_public_key, 
                                            conn_done_sender,
                                            backoff_ticks,
                                            client_connector,
                                            encrypt_transform,
                                            spawner.clone());

    let incoming_conn_done = incoming_conn_done
        .map(|opt_conn| CpEvent::<B>::ConnectAttemptDone(opt_conn));

    let incoming_requests = incoming_requests
        .map(|connect_request| CpEvent::<B>::ConnectRequest(connect_request))
        .chain(stream::once(future::ready(CpEvent::ConnectRequestClosed)));

    let incoming_config = incoming_config
        .map(|config_request| CpEvent::ConfigRequest(config_request))
        .chain(stream::once(future::ready(CpEvent::ConfigRequestClosed)));

    let incoming_ticks = timer_stream
        .map(|_| CpEvent::TimerTick)
        .chain(stream::once(future::ready(CpEvent::TimerClosed)));

    let mut incoming_events = incoming_conn_done
        .select(incoming_requests)
        .select(incoming_config)
        .select(incoming_ticks);

    while let Some(event) = await!(incoming_events.next()) {
        match event {
            CpEvent::ConnectRequest(connect_request) => 
                connect_pool.handle_connect_request(connect_request)?,
            CpEvent::ConnectRequestClosed => return Err(ConnectPoolError::ConnectRequestClosed),
            CpEvent::ConfigRequest(config) => 
                connect_pool.handle_config_request(config)?,
            CpEvent::ConfigRequestClosed => return Err(ConnectPoolError::ConfigRequestClosed),
            CpEvent::TimerTick => 
                connect_pool.handle_timer_tick()?,
            CpEvent::TimerClosed => return Err(ConnectPoolError::TimerClosed),
            CpEvent::ConnectAttemptDone(opt_conn) => 
                connect_pool.handle_connect_attempt_done(opt_conn),
        }
    }
    Ok(())
}

pub type ConnectPoolControl<B> = (CpConfigClient<B>, CpConnectClient);

#[allow(unused)]
pub fn create_connect_pool<B,ET,TS,C,S>(timer_stream: TS,
                            mut encrypt_transform: ET,
                            friend_public_key: PublicKey,
                            backoff_ticks: usize,
                            client_connector: C,
                            mut spawner: S) 
    -> Result<ConnectPoolControl<B>, ConnectPoolError> 

where
    B: Hash + Clone + Eq + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
    TS: Stream + Unpin + Send + 'static,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let (connect_request_sender, incoming_requests) = mpsc::channel(0);
    let (config_request_sender, incoming_config) = mpsc::channel(0);

    let loop_fut = connect_pool_loop(incoming_requests,
                            incoming_config,
                            timer_stream,
                            encrypt_transform,
                            friend_public_key,
                            backoff_ticks,
                            client_connector,
                            spawner.clone())
        .map_err(|e| error!("connect_pool_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut)
        .map_err(|_| ConnectPoolError::SpawnError)?;


    Ok((CpConfigClient::new(config_request_sender),
        CpConnectClient::new(connect_request_sender)))
}

#[derive(Clone)]
pub struct PoolConnector<B,C,ET,S> {
    timer_client: TimerClient,
    client_connector: C,
    encrypt_transform: ET,
    backoff_ticks: usize,
    spawner: S,
    phantom_b: PhantomData<B>,
}

impl<B,C,ET,S> PoolConnector<B,C,ET,S> 
where
    B: Hash + Clone + Eq + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    pub fn new(timer_client: TimerClient,
           client_connector: C,
           encrypt_transform: ET,
           backoff_ticks: usize,
           spawner: S) -> Self {

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


impl<B,C,ET,S> FutTransform for PoolConnector<B,C,ET,S> 
where
    B: Hash + Clone + Eq + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Input = PublicKey;
    type Output = ConnectPoolControl<B>;

    fn transform(&mut self, friend_public_key: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            // TODO: Should we keep the unwrap()-s here?
            let timer_stream = await!(self.timer_client.request_timer_stream()).unwrap();
            create_connect_pool(timer_stream,
                            self.encrypt_transform.clone(),
                            friend_public_key,
                            self.backoff_ticks,
                            self.client_connector.clone(),
                            self.spawner.clone()).unwrap()
        })
    }
}
