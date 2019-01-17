use std::collections::{HashMap, HashSet};
use futures::{future, stream, Stream, StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};

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

struct ListenPool<B> {
    local_address: Vec<B>,
    // friends: HashMap<PublicKey, Vec<B>>,
    relays: HashMap<B, Relay>,
}

#[derive(Debug)]
enum ListenPoolError {
    ConfigClosed,
    TimerClosed,
    SpawnError,
}

enum LpEvent<B> {
    Config(LpConfig<B>),
    ConfigClosed,
    IncomingConn((PublicKey, RawConn)), // Incoming encrypted connection
    TimerTick,
    TimerClosed,
}

async fn listen_pool_loop<B,L,ET,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                                 listener: L,
                                 encrypt_transform: ET,
                                 backoff_ticks: usize,
                                 timer_stream: TS,
                                 spawner: S) -> Result<(), ListenPoolError>
where
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    ET: FutTransform<Input=(Option<PublicKey>, RawConn), Output=Option<RawConn>> + Clone + Send + 'static,
    TS: Stream + Unpin,
    S: Spawn,
{

    let (conn_sender, incoming_conn) = mpsc::channel(0);

    let incoming_conn = incoming_conn
        .map(|in_conn| LpEvent::IncomingConn(in_conn));

    let incoming_config = incoming_config
        .map(|config| LpEvent::Config(config))
        .chain(stream::once(future::ready(LpEvent::ConfigClosed)));

    let timer_stream = timer_stream
        .map(|_| LpEvent::<B>::TimerTick)
        .chain(stream::once(future::ready(LpEvent::TimerClosed)));

    let mut incoming_events = incoming_conn
        .select(incoming_config)
        .select(timer_stream);

    while let Some(event) = await!(incoming_events.next()) {
    }

    unimplemented!();
}

