#![allow(unused)]
use std::collections::HashMap;
use bytes::Bytes;
use futures::{Stream, Sink};
use futures::prelude::{async, await};
use crypto::identity::PublicKey;
use timer::TimerClient;

mod state;


struct ConnPair {
    receiver: Box<Stream<Item=Bytes, Error=()>>,
    sender: Box<Sink<SinkItem=Bytes, SinkError=()>>,
}

impl ConnPair {
    fn new<M,K>(receiver: M, sender: K) -> ConnPair 
    where
        M: Stream<Item=Bytes, Error=()> + 'static,
        K: Sink<SinkItem=Bytes, SinkError=()> + 'static,
    {
        ConnPair {
            receiver: Box::new(receiver),
            sender: Box::new(sender),
        }
    }

    fn into(self) -> (Box<Stream<Item=Bytes, Error=()>>,
                      Box<Sink<SinkItem=Bytes, SinkError=()>>) {
        (self.receiver, self.sender)
    }

}

struct HalfTunnel {
    conn_pair: ConnPair,
    ticks_to_close: usize,
}

struct Listener {
    half_tunnel: HashMap<PublicKey, HalfTunnel>,
    conn_pair: ConnPair,
    ticks_to_close: usize,
    ticks_to_send_keepalive: usize,
}

pub struct RelayServer {
    listeners: HashMap<PublicKey, Listener>,
    timer_client: TimerClient,
    keepalive_ticks: usize,
}


enum IncomingConnInner {
    Listen(ConnPair),
    Accept((ConnPair, PublicKey)),
    Connect((ConnPair, PublicKey)),
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


#[async]
fn tunnel_loop(conn_pair1: ConnPair, 
               conn_pair2: ConnPair, 
               timer_client: TimerClient) -> Result<(), RelayServerError> {
    let (receiver1, sender1) = conn_pair1.into();
    let (receiver2, sender2) = conn_pair2.into();
    unimplemented!();
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

    fn incoming_conn(&mut self, 
                     conn_pair: ConnPair, 
                     public_key: PublicKey) {
    }


    #[async]
    fn run(self) -> Result<!, RelayServerError> {
        unimplemented!();
    }
}
