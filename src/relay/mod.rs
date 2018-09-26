#![allow(unused)]
use std::collections::HashMap;
use bytes::Bytes;
use futures::{stream, Stream, Sink};
use futures::prelude::{async, await}; use crypto::identity::PublicKey;
use timer::TimerClient;

use self::messages::{TunnelMessage, RelayListenIn, RelayListenOut};

mod messages;
mod tunnel;


pub struct ConnPair<T,W> {
    receiver: Box<Stream<Item=T, Error=()>>,
    sender: Box<Sink<SinkItem=W, SinkError=()>>,
}

impl<T,W> ConnPair<T,W> {
    pub fn new<M,K>(receiver: M, sender: K) -> ConnPair<T,W> 
    where
        M: Stream<Item=T, Error=()> + 'static,
        K: Sink<SinkItem=W, SinkError=()> + 'static,
    {
        ConnPair {
            receiver: Box::new(receiver),
            sender: Box::new(sender),
        }
    }

    pub fn into_inner(self) -> (Box<Stream<Item=T, Error=()>>,
                      Box<Sink<SinkItem=W, SinkError=()>>) {
        (self.receiver, self.sender)
    }

}

struct HalfTunnel {
    conn_pair: ConnPair<TunnelMessage, TunnelMessage>,
    ticks_to_close: usize,
}

struct Listener {
    half_tunnel: HashMap<PublicKey, HalfTunnel>,
    conn_pair: ConnPair<RelayListenIn, RelayListenOut>,
    ticks_to_close: usize,
    ticks_to_send_keepalive: usize,
}

pub struct RelayServer {
    listeners: HashMap<PublicKey, Listener>,
    timer_client: TimerClient,
    keepalive_ticks: usize,
}


enum IncomingConnInner {
    Listen(ConnPair<RelayListenIn, RelayListenOut>),
    Accept((ConnPair<TunnelMessage, TunnelMessage>, PublicKey)),
    Connect((ConnPair<TunnelMessage, TunnelMessage>, PublicKey)),
}

struct IncomingConn {
    conn_public_key: PublicKey,
    inner: IncomingConnInner,
}

enum RelayServerEvent {
    IncomingConn(IncomingConn),
    ConnClosed((PublicKey, PublicKey)), // Connection to pk1 (listener) from pk2 (initiator) was closed
}

enum RelayServerError {
}

impl RelayServer {
    fn new(timer_client: TimerClient, 
           keepalive_ticks: usize) -> RelayServer {
        RelayServer {
            listeners: HashMap::new(),
            timer_client,
            keepalive_ticks,
        }
    }

    #[async]
    fn tick(self) -> Result<Self, RelayServerError> {
        // TODO:
        // - Decrease various tick counters.
        // - Send keepalive messages if required for:
        //      - Listeners
        // - Remove if required:
        //      - Listeners
        //      - HalfConns
        unimplemented!();
    }


    #[async]
    fn run(self) -> Result<!, RelayServerError> {
        unimplemented!();
    }
}
