use std::marker::Unpin;
use std::collections::VecDeque;

use futures::channel::{mpsc, oneshot};
use futures::{select, future, FutureExt, TryFutureExt, stream, Stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPair, FutTransform};
use timer::TimerClient;

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
enum ConnectPoolError {
    SpawnError,
    ConnectRequestClosed,
    ConfigRequestClosed,
    TimerClosed,
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
    Empty,
    Waiting((usize, oneshot::Sender<RawConn>)),
    Connecting((B, oneshot::Sender<()>, oneshot::Sender<RawConn>)),
}

struct ConnectPool<B,C,ET,S> {
    addresses: VecDeque<B>,
    status: CpStatus<B>,
    conn_done_sender: mpsc::Sender<Option<RawConn>>,
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
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>>,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>>,
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
    B: Eq,
    S: Spawn,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>>,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>>,
{
    pub fn new(addresses: impl Into<VecDeque<B>>,
               conn_done_sender: mpsc::Sender<Option<RawConn>>,
               client_connector: C,
               encrypt_transform: ET,
               spawner: S) -> Self {

        ConnectPool {
            addresses: addresses.into(),
            status: CpStatus::Empty,
            conn_done_sender,
            client_connector,
            encrypt_transform,
            spawner,
        }
    }

    pub fn handle_connect_request(&mut self, connect_request: CpConnectRequest) {
        if let CpStatus::Empty = self.status {
        } else {
            panic!("ConnectPool::handle_connect_request(): We already have a connection attempt in progress!")
        }
        unimplemented!();
    }

    pub fn handle_config_request(&mut self, config: CpConfig<B>) {
        // TODO:
        match config {
            CpConfig::AddAddress(address) => self.addresses.push_back(address),
            CpConfig::RemoveAddress(address) => {
                self.addresses.retain(|cur_address| cur_address != &address);
            }
        }
        unimplemented!();
    }

    pub fn handle_timer_tick(&mut self) {
        let waiting = match &mut self.status {
            CpStatus::Empty |
            CpStatus::Connecting(_) => return,
            CpStatus::Waiting(waiting) => waiting,
        };

        unimplemented!();
    }

    pub fn handle_connect_attempt_done(&mut self, opt_conn: Option<RawConn>) {
        match self.status {
            CpStatus::Empty | 
            CpStatus::Waiting(_) => unreachable!(),
            CpStatus::Connecting(_) => {
                match opt_conn {
                    Some(raw_conn) => {},
                    None => {
                        unimplemented!();
                    },
                }
            }
        }
    }
}


async fn connect_pool_loop<B,ET,TS,C,S>(incoming_requests: mpsc::Receiver<CpConnectRequest>,
                            incoming_config: mpsc::Receiver<CpConfig<B>>,
                            timer_stream: TS,
                            mut encrypt_transform: ET,
                            friend_public_key: PublicKey,
                            addresses: Vec<B>,
                            timer_client: TimerClient,
                            client_connector: C,
                            spawner: S) -> Result<(), ConnectPoolError>
where
    B: Eq,
    C: FutTransform<Input=(B, PublicKey), Output=Option<RawConn>>,
    TS: Stream + Unpin,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>>,
    S: Spawn + Clone,
{

    let (conn_done_sender, incoming_conn_done) = mpsc::channel(0);
    let mut connect_pool = ConnectPool::new(addresses,
                                            conn_done_sender,
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
                connect_pool.handle_connect_request(connect_request),
            ConnectPoolEvent::ConnectRequestClosed => return Err(ConnectPoolError::ConnectRequestClosed),
            ConnectPoolEvent::ConfigRequest(config) => 
                connect_pool.handle_config_request(config),
            ConnectPoolEvent::ConfigRequestClosed => return Err(ConnectPoolError::ConfigRequestClosed),
            ConnectPoolEvent::TimerTick => 
                connect_pool.handle_timer_tick(),
            ConnectPoolEvent::TimerClosed => return Err(ConnectPoolError::TimerClosed),
            ConnectPoolEvent::ConnectAttemptDone(opt_conn) => 
                connect_pool.handle_connect_attempt_done(opt_conn),
        }
    }
    Ok(())
}

/*
fn create_connect_pool<B,S>(friend_public_key: PublicKey, 
                         addresses: Vec<B>,
                         mut spawner: S) 
    -> Result<(CpConfigClient<B>, CpConnectClient),  ConnectPoolError>

where
    S: Spawn + Clone + 'static,
    B: 'static,
{
    let (connect_request_sender, incoming_requests) = mpsc::channel(0);
    let (config_request_sender, incoming_config) = mpsc::channel(0);

    let loop_fut = connect_pool_loop(incoming_requests,
                                incoming_config,
                                friend_public_key, 
                                addresses, 
                                spawner.clone())
        .map_err(|e| error!("connect_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut)
        .map_err(|_| ConnectPoolError::SpawnError)?;


    Ok((CpConfigClient::new(config_request_sender),
        CpConnectClient::new(connect_request_sender)))
}
*/
