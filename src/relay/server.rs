#![allow(unused)]
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::prelude::{async, await, async_stream}; 
use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use timer::TimerClient;

use super::messages::{InitConnection, TunnelMessage, 
    RelayListenIn, RelayListenOut, RejectConnection, IncomingConnection};
use super::types::{IncomingConn, IncomingConnInner, 
    IncomingListen, IncomingAccept, IncomingConnect};
use super::serialize::{deserialize_init_connection, deserialize_relay_listen_in,
                        serialize_relay_listen_out, serialize_tunnel_message,
                        deserialize_tunnel_message};
use super::listener::listener_keepalive;
use super::tunnel::tunnel_loop;



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

impl<M,K> ConnPair<M,K> {
    fn new(receiver: M, sender: K) -> Self {
        ConnPair {receiver, sender}
    }
}

struct HalfTunnel<MT,KT> {
    conn_pair: ConnPair<MT,KT>,
    ticks_to_close: usize,
}

struct Listener<K,MT,KT> {
    half_tunnels: HashMap<PublicKey, HalfTunnel<MT,KT>>,
    tunnels: HashSet<PublicKey>,
    sender: K
}

impl<K,MT,KT> Listener<K,MT,KT> {
    fn new(sender: K) -> Self {
        Listener {
            half_tunnels: HashMap::new(),
            tunnels: HashSet::new(),
            sender,
        }
    }
}

struct TunnelClosed {
    init_public_key: PublicKey,
    listen_public_key: PublicKey,
}

enum RelayServerEvent<ML,KL,MA,KA,MC,KC> {
    IncomingConn(IncomingConn<ML,KL,MA,KA,MC,KC>),
    IncomingConnsClosed,
    TunnelClosed(TunnelClosed),
    ListenerMessage((PublicKey, RejectConnection)),
    ListenerClosed(PublicKey),
    TimerTick,
}

enum RelayServerError {
    IncomingConnsError,
    RequestTimerStreamError,
    TimerStreamError,
    TimerClosedError,
    ListenerEventReceiverError,
    ListenerEventReceiverClosed,
    ListeningNotInProgress,
    NoPendingHalfTunnel,
    AlreadyListening,
    TunnelClosedReceiverError,
    TunnelClosedReceiverClosed,
}

/*
fn handle_listen<M,K,MT,KT,ML,KL>(listeners: &mut HashMap<PublicKey, Listener<K,MT,KT>>,
                            listener_public_key: PublicKey,
                            incoming_listen: IncomingListen<ML,KL>,
                            listener_event_sender: mpsc::Sender<(PublicKey, RejectConnection)>,
                            timer_client: TimerClient,
                            keepalive_ticks: usize,
                            handle: &Handle) -> Result<(), RelayServerError>
where
    M: Stream<Item=TunnelMessage,Error=()>,
    K: Sink<SinkItem=TunnelMessage,SinkError=()>,
    MT: Stream<Item=TunnelMessage,Error=()>,
    KT: Sink<SinkItem=TunnelMessage,SinkError=()>,
    ML: Stream<Item=RelayListenIn,Error=()>,
    KL: Sink<SinkItem=RelayListenOut,SinkError=()>,
{
    if listeners.contains_key(&listener_public_key) {
        return Err(RelayServerError::AlreadyListening); // Discard Listen connection
    }
    let (receiver, sender) = listener_keepalive(incoming_listen.receiver,
                          incoming_listen.sender,
                          timer_client.clone(),
                          keepalive_ticks,
                          &handle);
    
    let listener = Listener::new(sender);
    listeners.insert(listener_public_key, listener);
    let receiver = receiver
        .map(|reject_connection| (listener_public_key.clone(), reject_connection))
        .map_err(|_| ());
    handle.spawn(listener_event_sender
                 .sink_map_err(|_| ())
                 .send_all(receiver)
                 .then(|_| Ok(())));
    Ok(())
}
*/


fn handle_accept<K,MT,KT,MA,KA>(listeners: &mut HashMap<PublicKey, Listener<K,MT,KT>>,
                            acceptor_public_key: PublicKey,
                            incoming_accept: IncomingAccept<MA,KA>,
                            tunnel_closed_sender: mpsc::Sender<TunnelClosed>,
                            timer_client: TimerClient,
                            keepalive_ticks: usize,
                            handle: &Handle) -> Result<(), RelayServerError>
where
    K: Sink<SinkItem=IncomingConnection,SinkError=()> + 'static,
    MT: Stream<Item=TunnelMessage,Error=()> + 'static,
    KT: Sink<SinkItem=TunnelMessage,SinkError=()> + 'static,
    MA: Stream<Item=TunnelMessage,Error=()> + 'static,
    KA: Sink<SinkItem=TunnelMessage,SinkError=()> + 'static,
{
    let listener = match listeners.get_mut(&acceptor_public_key) {
        Some(listener) => listener,
        None => return Err(RelayServerError::ListeningNotInProgress),
    };
    let IncomingAccept {receiver, sender, accept_public_key} = incoming_accept;
    let conn_pair = 
        match listener.half_tunnels.remove(&accept_public_key) {
            Some(HalfTunnel {conn_pair, ..}) => conn_pair,
            None => return Err(RelayServerError::NoPendingHalfTunnel),
        };
    let c_accept_public_key = accept_public_key.clone();
    let tunnel_fut = tunnel_loop(conn_pair.receiver, conn_pair.sender,
                receiver, sender,
                timer_client.clone(),
                keepalive_ticks)
    .map_err(|e| {
        println!("tunnel_loop() error: {:?}", e);
        ()
    })
    .then(move |_| {
        let tunnel_closed = TunnelClosed {
            init_public_key: c_accept_public_key,
            listen_public_key: acceptor_public_key,
        };
        tunnel_closed_sender.clone()
            .send(tunnel_closed)
            .then(|_| Ok(()))
    });
    listener.tunnels.insert(accept_public_key);
    handle.spawn(tunnel_fut);
    Ok(())
}

 
#[async]
pub fn relay_server<ML,KL,MA,KA,MC,KC,S>(timer_client: TimerClient, 
                incoming_conns: S,
                keepalive_ticks: usize,
                handle: Handle) -> Result<(), RelayServerError> 
where
    ML: Stream<Item=RelayListenIn,Error=()>,
    KL: Sink<SinkItem=RelayListenOut,SinkError=()>,
    MA: Stream<Item=TunnelMessage,Error=()>,
    KA: Sink<SinkItem=TunnelMessage,SinkError=()>,
    MC: Stream<Item=TunnelMessage,Error=()>,
    KC: Sink<SinkItem=TunnelMessage,SinkError=()>,
    S: Stream<Item=IncomingConn<ML,KL,MA,KA,MC,KC>, Error=()> + 'static,
{

    let timer_stream = await!(timer_client.clone().request_timer_stream())
        .map_err(|_| RelayServerError::RequestTimerStreamError)?;
    let timer_stream = timer_stream
        .map_err(|_| RelayServerError::TimerStreamError)
        .map(|_| RelayServerEvent::TimerTick)
        .chain(stream::once(Err(RelayServerError::TimerClosedError)));

    let incoming_conns = incoming_conns
        .map_err(|_| RelayServerError::IncomingConnsError)
        .map(|incoming_conn| RelayServerEvent::IncomingConn(incoming_conn))
        .chain(stream::once(Ok(RelayServerEvent::IncomingConnsClosed)));

    let (listener_event_sender, listener_event_receiver) = mpsc::channel::<(PublicKey, RejectConnection)>(0);
    let listener_event_receiver = listener_event_receiver
        .map_err(|_| RelayServerError::ListenerEventReceiverError)
        .map(|(public_key, reject_connection)| RelayServerEvent::ListenerMessage((public_key, reject_connection)))
        .chain(stream::once(Err(RelayServerError::ListenerEventReceiverClosed)));

    let (tunnel_closed_sender, tunnel_closed_receiver) = mpsc::channel::<TunnelClosed>(0);
    let tunnel_closed_receiver = tunnel_closed_receiver
        .map_err(|_| RelayServerError::TunnelClosedReceiverError)
        .map(|tunnel_closed| RelayServerEvent::TunnelClosed(tunnel_closed))
        .chain(stream::once(Err(RelayServerError::TunnelClosedReceiverClosed)));

    let relay_server_events = timer_stream
        .select(incoming_conns)
        .select(listener_event_receiver)
        .select(tunnel_closed_receiver);

    let mut listeners: HashMap<PublicKey, Listener<_,_,_>> = HashMap::new();

    #[async]
    for relay_server_event in relay_server_events {
        let c_timer_client = timer_client.clone();
        match relay_server_event {
            RelayServerEvent::IncomingConn(incoming_conn) => {
                let IncomingConn {public_key, inner} = incoming_conn;
                match inner {
                    IncomingConnInner::Listen(incoming_listen) => {
                        if listeners.contains_key(&public_key) {
                            continue; // Discard Listen connection
                        }
                        let (receiver, sender) = listener_keepalive(incoming_listen.receiver,
                                              incoming_listen.sender,
                                              c_timer_client,
                                              keepalive_ticks,
                                              &handle);
                        
                        let listener = Listener::new(sender);
                        listeners.insert(public_key.clone(), listener);
                        let receiver = receiver
                            .map(move |reject_connection| (public_key.clone(), reject_connection))
                            .map_err(|_| ());
                        handle.spawn(listener_event_sender.clone()
                                     .sink_map_err(|_| ())
                                     .send_all(receiver)
                                     .then(|_| Ok(())));
                    },
                    IncomingConnInner::Accept(incoming_accept) => {
                        let _ = handle_accept(&mut listeners,
                                      public_key.clone(),
                                      incoming_accept,
                                      tunnel_closed_sender.clone(),
                                      c_timer_client,
                                      keepalive_ticks,
                                      &handle);
                    },
                    IncomingConnInner::Connect(incoming_connect) => {
                        let listener = match listeners.get_mut(&incoming_connect.connect_public_key) {
                            Some(listener) => listener,
                            None => continue, // Discard Connect connection
                        };
                        let half_tunnel = HalfTunnel {
                            conn_pair: ConnPair::new(incoming_connect.receiver, 
                                                     incoming_connect.sender),
                            ticks_to_close: keepalive_ticks,
                        };
                        listener.half_tunnels.insert(public_key.clone(), half_tunnel);
                        unimplemented!()
                    },
                }
            },
            RelayServerEvent::IncomingConnsClosed => {},
            RelayServerEvent::TunnelClosed(tunnel_closed) => {},
            RelayServerEvent::ListenerMessage((public_key, RejectConnection(rejected_public_key))) => {},
            RelayServerEvent::ListenerClosed(public_key) => {},
            RelayServerEvent::TimerTick => {},
        }

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
