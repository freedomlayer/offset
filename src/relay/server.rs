#![allow(unused)]
use std::collections::HashMap;
use std::marker::PhantomData;
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::prelude::{async, await, async_stream}; 
use crypto::identity::PublicKey;
use timer::TimerClient;

use super::messages::{InitConnection, TunnelMessage, 
    RelayListenIn, RelayListenOut, RejectConnection};
use super::types::{IncomingConn, IncomingConnInner, 
    IncomingListen, IncomingAccept, IncomingConnect};
use super::serialize::{deserialize_init_connection, deserialize_relay_listen_in,
                        serialize_relay_listen_out, serialize_tunnel_message,
                        deserialize_tunnel_message};



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
}

pub struct RelayServer<M,K,MT,KT> {
    listeners: HashMap<PublicKey, Listener<M,K,MT,KT>>,
    timer_client: TimerClient,
    keepalive_ticks: usize, // config
}

struct TunnelClosed {
    initiator: PublicKey,
    listener: PublicKey,
}

enum RelayServerEvent<ML,KL,MA,KA,MC,KC> {
    IncomingConn(IncomingConn<ML,KL,MA,KA,MC,KC>),
    IncomingConsClosed,
    TunnelClosed(TunnelClosed),
    ListenerMessage((PublicKey, RejectConnection)),
    ListenerClosed(PublicKey),
    TimerTick,
}

enum RelayServerError {
    IncomingConnsClosed,
    IncomingConnsError,
    RequestTimerStreamError,
    TimerStreamError,
    TimerClosedError,
    ListenerEventReceiverError,
    ListenerEventReceiverClosed,
}


 
#[async]
fn relay_server<ML,KL,MA,KA,MC,KC,S>(timer_client: TimerClient, 
                incoming_conns: S,
                keepalive_ticks: usize) -> Result<!, RelayServerError> 
where
    ML: Stream<Item=RelayListenIn,Error=()>,
    KL: Sink<SinkItem=RelayListenOut,SinkError=()>,
    MA: Stream<Item=TunnelMessage,Error=()>,
    KA: Sink<SinkItem=TunnelMessage,SinkError=()>,
    MC: Stream<Item=TunnelMessage,Error=()>,
    KC: Sink<SinkItem=TunnelMessage,SinkError=()>,
    S: Stream<Item=IncomingConn<ML,KL,MA,KA,MC,KC>, Error=()> + 'static,
{

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| RelayServerError::RequestTimerStreamError)?;
    let timer_stream = timer_stream
        .map_err(|_| RelayServerError::TimerStreamError)
        .map(|_| RelayServerEvent::TimerTick)
        .chain(stream::once(Err(RelayServerError::TimerClosedError)));

    let incoming_conns = incoming_conns
        .map_err(|_| RelayServerError::IncomingConnsError)
        .map(|incoming_conn| RelayServerEvent::IncomingConn(incoming_conn))
        .chain(stream::once(Err(RelayServerError::IncomingConnsClosed)));

    let (listener_event_sender, listener_event_receiver) = mpsc::channel::<(PublicKey, RejectConnection)>(0);

    let listener_event_receiver = listener_event_receiver
        .map_err(|_| RelayServerError::ListenerEventReceiverError)
        .map(|(public_key, reject_connection)| RelayServerEvent::ListenerMessage((public_key, reject_connection)))
        .chain(stream::once(Err(RelayServerError::ListenerEventReceiverClosed)));

    let relay_server_events = timer_stream
        .select(incoming_conns)
        .select(listener_event_receiver);

    #[async]
    for relay_sever_event in relay_server_events {
    // TODO:
    // check for any event:
    // - Incoming connection 
    // - A connection was closed
    //      - Remove from data structures
    // - Time tick
    //      - Possibly timeout half tunnels
    }
    unreachable!();
}
