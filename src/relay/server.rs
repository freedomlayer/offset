#![allow(unused)]
use std::collections::HashMap;
use std::marker::PhantomData;
use bytes::Bytes;
use futures::{stream, Stream, Sink};
use futures::prelude::{async, await}; use crypto::identity::PublicKey;
use timer::TimerClient;

use super::messages::{TunnelMessage, RelayListenIn, RelayListenOut};

struct ConnPair<M,K> {
    receiver: M,
    sender: K,
}

struct HalfTunnel<MT,KT> {
    conn_pair: ConnPair<MT,KT>,
    ticks_to_close: usize,
}

struct Listener<M,K,MT,KT> {
    half_tunnel: HashMap<PublicKey, HalfTunnel<MT,KT>>,
    conn_pair: ConnPair<M,K>,
    ticks_to_close: usize,
    ticks_to_send_keepalive: usize,
}

pub struct RelayServer<M,K,MT,KT> {
    listeners: HashMap<PublicKey, Listener<M,K,MT,KT>>,
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

/*
impl<M,K,MT,KT> RelayServer<M,K,MT,KT> {
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
        // TODO:
        // check for any event:
        // - Incoming connection 
        //      (sender, receiver) pair an a public key
        //      - Convert the connection into one of three: Listen, Accept or Connect.
        //          (Should be done using a .map() adapter on the Stream).
        //          
        // - A connection was closed
        //      - Remove from data structures
        // - Time tick
        //      - Possibly timeout: Listening conn
        //      - Send keepalive if required to a listening conn.
        unimplemented!();
    }
}

*/
 
fn relay_server<T,M,K>(timer_client: TimerClient, 
                incoming_conns: T,
                keepalive_ticks: usize) -> Result<!, RelayServerError> 
where
    T: Stream<Item=(M, K), Error=()>,
    M: Stream<Item=Bytes, Error=()>,
    K: Sink<SinkItem=Bytes, SinkError=()>,
{
    // TODO:
    // check for any event:
    // - Incoming connection 
    //      (sender, receiver) pair an a public key
    //      - Convert the connection into one of three: Listen, Accept or Connect.
    //          (Should be done using a .map() adapter on the Stream).
    //          
    // - A connection was closed
    //      - Remove from data structures
    // - Time tick
    //      - Possibly timeout: Listening conn
    //      - Send keepalive if required to a listening conn.
    unimplemented!();
}
