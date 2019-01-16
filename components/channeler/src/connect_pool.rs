use std::mem;
use std::marker::Unpin;
use std::collections::VecDeque;

use futures::channel::{mpsc, oneshot};
use futures::{select, future, FutureExt, TryFutureExt, stream, Stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPair, FutTransform};

use crypto::identity::PublicKey;

type RawConn = ConnPair<Vec<u8>,Vec<u8>>;

#[derive(Debug)]
struct ConnectPoolClientError;

#[derive(Debug, Clone)]
pub enum CpConfig<B> {
    AddAddress(B),
    RemoveAddress(B),
}

pub struct CpConnectRequest {
    response_sender: oneshot::Sender<RawConn>,
}


pub struct CpConnectClient {
    request_sender: mpsc::Sender<CpConnectRequest>,
}

impl CpConnectClient {
    pub fn new(request_sender: mpsc::Sender<CpConnectRequest>) -> Self {
        CpConnectClient {
            request_sender,
        }
    }
}

pub struct CpConfigClient<B> {
    request_sender: mpsc::Sender<CpConfig<B>>,
}

impl<B> CpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<CpConfig<B>>) -> Self {
        CpConfigClient {
            request_sender,
        }
    }
}

impl CpConnectClient {
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

impl<B> CpConfigClient<B> {
    pub async fn config(&mut self, config: CpConfig<B>) -> Result<(), ConnectPoolClientError> {
        await!(self.request_sender.send(config))
            .map_err(|_| ConnectPoolClientError)?;
        Ok(())
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

enum ConnectPoolEvent<B> {
    ConnectRequest(CpConnectRequest),
    ConnectRequestClosed,
    ConfigRequest(CpConfig<B>),
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
    B: Clone + Eq + Send + 'static,
    S: Spawn,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
{
    pub fn new(friend_public_key: PublicKey, 
               addresses: impl Into<VecDeque<B>>,
               conn_done_sender: mpsc::Sender<Option<RawConn>>,
               backoff_ticks: usize,
               client_connector: C,
               encrypt_transform: ET,
               spawner: S) -> Self {

        ConnectPool {
            friend_public_key,
            addresses: addresses.into(),
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

    pub fn handle_config_request(&mut self, config: CpConfig<B>) 
        -> Result<(), ConnectPoolError> {

        match config {
            CpConfig::AddAddress(address) => {
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
            },
            CpConfig::RemoveAddress(address) => {
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
                }
            }
        };

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
                            incoming_config: mpsc::Receiver<CpConfig<B>>,
                            timer_stream: TS,
                            encrypt_transform: ET,
                            friend_public_key: PublicKey,
                            addresses: Vec<B>,
                            backoff_ticks: usize,
                            client_connector: C,
                            spawner: S) -> Result<(), ConnectPoolError>
where
    B: Clone + Eq + Send + 'static,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    S: Spawn + Clone,
{

    let (conn_done_sender, incoming_conn_done) = mpsc::channel(0);
    let mut connect_pool = ConnectPool::new(friend_public_key, 
                                            addresses,
                                            conn_done_sender,
                                            backoff_ticks,
                                            client_connector,
                                            encrypt_transform,
                                            spawner.clone());

    let incoming_conn_done = incoming_conn_done
        .map(|opt_conn| ConnectPoolEvent::<B>::ConnectAttemptDone(opt_conn));

    let incoming_requests = incoming_requests
        .map(|connect_request| ConnectPoolEvent::<B>::ConnectRequest(connect_request))
        .chain(stream::once(future::ready(ConnectPoolEvent::ConnectRequestClosed)));

    let incoming_config = incoming_config
        .map(|config_request| ConnectPoolEvent::ConfigRequest(config_request))
        .chain(stream::once(future::ready(ConnectPoolEvent::ConfigRequestClosed)));

    let incoming_ticks = timer_stream
        .map(|_| ConnectPoolEvent::TimerTick)
        .chain(stream::once(future::ready(ConnectPoolEvent::TimerClosed)));

    let mut incoming_events = incoming_conn_done
        .select(incoming_requests)
        .select(incoming_config)
        .select(incoming_ticks);

    while let Some(event) = await!(incoming_events.next()) {
        match event {
            ConnectPoolEvent::ConnectRequest(connect_request) => 
                connect_pool.handle_connect_request(connect_request)?,
            ConnectPoolEvent::ConnectRequestClosed => return Err(ConnectPoolError::ConnectRequestClosed),
            ConnectPoolEvent::ConfigRequest(config) => 
                connect_pool.handle_config_request(config)?,
            ConnectPoolEvent::ConfigRequestClosed => return Err(ConnectPoolError::ConfigRequestClosed),
            ConnectPoolEvent::TimerTick => 
                connect_pool.handle_timer_tick()?,
            ConnectPoolEvent::TimerClosed => return Err(ConnectPoolError::TimerClosed),
            ConnectPoolEvent::ConnectAttemptDone(opt_conn) => 
                connect_pool.handle_connect_attempt_done(opt_conn),
        }
    }
    Ok(())
}

#[allow(unused)]
pub fn create_connect_pool<B,ET,TS,C,S>(timer_stream: TS,
                            mut encrypt_transform: ET,
                            friend_public_key: PublicKey,
                            addresses: Vec<B>,
                            backoff_ticks: usize,
                            client_connector: C,
                            mut spawner: S) 
    -> Result<(CpConfigClient<B>, CpConnectClient), ConnectPoolError> 

where
    B: Clone + Eq + Send + 'static,
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
                            addresses,
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
