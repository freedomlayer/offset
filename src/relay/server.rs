#![allow(unused)]
use std::collections::HashMap;
use std::marker::PhantomData;
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::prelude::{async, await, async_stream}; 
use crypto::identity::PublicKey;
use timer::TimerClient;

use super::messages::{InitConnection, TunnelMessage, 
    RelayListenIn, RelayListenOut};
use super::serialize::{deserialize_init_connection, deserialize_relay_listen_in,
                        serialize_relay_listen_out, serialize_tunnel_message,
                        deserialize_tunnel_message};


// M: Stream<Item=RelayListenIn, Error=()>,
// K: Sink<SinkItem=RelayListenOut, SinkError=()>,
pub struct IncomingListen<M,K> {
    receiver: M,
    sender: K,
}

// M: Stream<Item=TunnelMessage, Error=()>>,
// K: Sink<SinkItem=TunnelMessage, SinkError=()>>,
pub struct IncomingAccept<M,K> {
    receiver: M,
    sender: K,
    accept_public_key: PublicKey,
}

// M: Stream<Item=TunnelMessage, Error=()>,
// K: SinkItem=TunnelMessage, SinkError=()>,
pub struct IncomingConnect<M,K> {
    receiver: M,
    sender: K,
    connect_public_key: PublicKey,
}

enum IncomingConnInner<ML,KL,MA,KA,MC,KC> {
    Listen(IncomingListen<ML,KL>),
    Accept(IncomingAccept<MA,KA>),
    Connect(IncomingConnect<MC,KC>),
}

struct IncomingConn<ML,KL,MA,KA,MC,KC> {
    public_key: PublicKey,
    inner: IncomingConnInner<ML,KL,MA,KA,MC,KC>,
}

fn dispatch_conn<M,K>(receiver: M, sender: K, public_key: PublicKey, first_msg: Vec<u8>) 
    -> Option<IncomingConn<impl Stream<Item=RelayListenIn,Error=()>,
                              impl Sink<SinkItem=RelayListenOut,SinkError=()>,
                              impl Stream<Item=TunnelMessage,Error=()>,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()>,
                              impl Stream<Item=TunnelMessage,Error=()>,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()>>>
where
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    let inner = match deserialize_init_connection(&first_msg).ok()? {
        InitConnection::Listen => {
            IncomingConnInner::Listen(IncomingListen {
                receiver: receiver.and_then(|data| deserialize_relay_listen_in(&data).map_err(|_| ())),
                sender: sender.with(|msg| Ok(serialize_relay_listen_out(&msg))),
            })
        },
        InitConnection::Accept(accept_public_key) => 
            IncomingConnInner::Accept(IncomingAccept {
                receiver: receiver.and_then(|data| deserialize_tunnel_message(&data).map_err(|_| ())),
                sender: sender.with(|msg| Ok(serialize_tunnel_message(&msg))),
                accept_public_key,
            }),
        InitConnection::Connect(connect_public_key) => 
            IncomingConnInner::Connect(IncomingConnect {
                receiver: receiver.and_then(|data| deserialize_tunnel_message(&data).map_err(|_| ())),
                sender: sender.with(|msg| Ok(serialize_tunnel_message(&msg))),
                connect_public_key,
            }),
    };

    Some(IncomingConn {
        public_key,
        inner,
    })
}


fn conn_processor<T,M,K>(timer_client: TimerClient,
                    incoming_conns: T,
                    keepalive_ticks: usize) -> impl Stream<
                        Item=IncomingConn<impl Stream<Item=RelayListenIn,Error=()>,
                                          impl Sink<SinkItem=RelayListenOut,SinkError=()>,
                                          impl Stream<Item=TunnelMessage,Error=()>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>,
                                          impl Stream<Item=TunnelMessage,Error=()>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>>,
                        Error=()>
where
    T: Stream<Item=(M, K, PublicKey), Error=()>,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    incoming_conns.and_then(|(receiver, sender, public_key)| {
        receiver
            .into_future()
            .then(|res| {
                Ok(match res {
                    Ok((opt_first_msg, receiver)) => {
                        match opt_first_msg {
                            Some(first_msg) => dispatch_conn(receiver, sender, public_key, first_msg),
                            None => None,
                        }
                    },
                    Err(_) => None,
                })
            })
    }).filter_map(|opt_conn| opt_conn)
}

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

struct TunnelClosed {
    initiator: PublicKey,
    listener: PublicKey,
}

enum RelayServerEvent<ML,KL,MA,KA,MC,KC> {
    IncomingConn(IncomingConn<ML,KL,MA,KA,MC,KC>),
    TunnelClosed(TunnelClosed),
    ListenerMessage((PublicKey, RelayListenIn)),
    ListenerClosed(PublicKey),
}

enum RelayServerError {
    IncomingConnsClosed,
    IncomingConnsError,
}

 
#[async]
fn relay_server<T,M,K>(timer_client: TimerClient, 
                incoming_conns: T,
                keepalive_ticks: usize) -> Result<!, RelayServerError> 
where
    T: Stream<Item=(M, K, PublicKey), Error=()>,
    M: Stream<Item=Vec<u8>, Error=()>,
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{

    // TODO:
    // check for any event:
    // - Incoming connection 
    //      (sender, receiver) pair an a public key
    //      - Convert the connection into one of three: Listen, Accept or Connect.
    //          
    // - A connection was closed
    //      - Remove from data structures
    // - Time tick
    //      - Possibly timeout: Listening conn
    //      - Send keepalive if required to a listening conn.
    unimplemented!();
}
