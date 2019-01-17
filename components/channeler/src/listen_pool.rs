use std::hash::Hash;
use std::collections::{HashMap, HashSet};
use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};
use crate::transform_pool::create_transform_pool;

pub enum LpConfig<B> {
    SetLocalAddress(Vec<B>),
    UpdateFriend((PublicKey, Vec<B>)),
    RemoveFriend(PublicKey),
}

#[derive(Debug)]
struct ListenPoolClientError;


pub struct LpConfigClient<B> {
    request_sender: mpsc::Sender<LpConfig<B>>,
}

impl<B> LpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<LpConfig<B>>) -> Self {
        LpConfigClient {
            request_sender,
        }
    }

    pub async fn config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolClientError> {
        await!(self.request_sender.send(config))
            .map_err(|_| ListenPoolClientError)?;
        Ok(())
    }
}

enum RelayStatus {
    Waiting(usize), // ticks left to start listening again
    Connected(mpsc::Sender<AccessControlOpPk>),
}

struct Relay {
    friends: HashSet<PublicKey>,
    status: RelayStatus,
}

struct ListenPool<B,L,S> {
    local_address: Vec<B>,
    // friends: HashMap<PublicKey, Vec<B>>,
    relays: HashMap<B, Relay>,
    plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
    listener: L,
    backoff_ticks: usize,
    spawner: S,
}

#[derive(Debug)]
enum ListenPoolError {
    ConfigClosed,
    TimerClosed,
    SpawnError,
    CreateEncryptPoolError,
}

enum LpEvent<B> {
    Config(LpConfig<B>),
    ConfigClosed,
    IncomingEncryptedConn((PublicKey, RawConn)), // Incoming encrypted connection
    RelayClosed(B),
    TimerTick,
    TimerClosed,
}

impl<B,L,S> ListenPool<B,L,S> 
where
    B: Hash + Eq,
{
    fn new(plain_conn_sender: mpsc::Sender<(PublicKey, RawConn)>,
           listener: L,
           backoff_ticks: usize,
           spawner: S) -> Self {

        ListenPool {
            local_address: Vec::new(),
            relays: HashMap::new(),
            plain_conn_sender,
            listener,
            backoff_ticks,
            spawner,
        }
    }

    fn handle_config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolError> {
        unimplemented!();
    }

    fn handle_incoming_encrypted_conn(&mut self,
                                      remote_public_key: PublicKey,
                                      enc_conn: RawConn) -> Result<(), ListenPoolError> {
        unimplemented!();
    }

    fn handle_relay_closed(&mut self,
                           address: B) -> Result<(), ListenPoolError> {
        unimplemented!();
    }

    fn handle_timer_tick(&mut self) -> Result<(), ListenPoolError> {
        unimplemented!();
    }
}


async fn listen_pool_loop<B,L,ET,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                                 listener: L,
                                 encrypt_transform: ET,
                                 max_concurrent_encrypt: usize,
                                 backoff_ticks: usize,
                                 timer_stream: TS,
                                 spawner: S) -> Result<(), ListenPoolError>
where
    B: Eq + Hash,
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    ET: FutTransform<Input=(PublicKey, RawConn), Output=Option<(PublicKey, RawConn)>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    S: Spawn + Clone + Send + 'static,
{

    let (plain_conn_sender, incoming_encrypted_conn) = create_transform_pool(
                          encrypt_transform, 
                          max_concurrent_encrypt,
                          spawner.clone())
        .map_err(|_| ListenPoolError::CreateEncryptPoolError)?;

    let mut listen_pool = ListenPool::<B,L,S>::new(plain_conn_sender,
                                      listener,
                                      backoff_ticks,
                                      spawner);

    let incoming_encrypted_conn = incoming_encrypted_conn
        .map(|pk_enc_conn| LpEvent::IncomingEncryptedConn(pk_enc_conn));

    let incoming_config = incoming_config
        .map(|config| LpEvent::Config(config))
        .chain(stream::once(future::ready(LpEvent::ConfigClosed)));

    let timer_stream = timer_stream
        .map(|_| LpEvent::<B>::TimerTick)
        .chain(stream::once(future::ready(LpEvent::TimerClosed)));

    let mut incoming_events = incoming_encrypted_conn
        .select(incoming_config)
        .select(timer_stream);

    while let Some(event) = await!(incoming_events.next()) {
        match event {
            LpEvent::Config(config) => 
                listen_pool.handle_config(config)?,
            LpEvent::ConfigClosed => return Err(ListenPoolError::ConfigClosed),
            LpEvent::IncomingEncryptedConn((remote_public_key, enc_conn)) => 
                listen_pool.handle_incoming_encrypted_conn(remote_public_key, enc_conn)?,
            LpEvent::RelayClosed(address) => 
                listen_pool.handle_relay_closed(address)?,
            LpEvent::TimerTick => 
                listen_pool.handle_timer_tick()?,
            LpEvent::TimerClosed => return Err(ListenPoolError::TimerClosed),
        }
    }
    Ok(())
}

