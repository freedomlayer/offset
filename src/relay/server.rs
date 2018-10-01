#![allow(unused)]
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::iter;

use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::prelude::{async, await, async_stream}; 
use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use timer::{TimerTick, TimerClient};
use utils::int_convert::usize_to_u64;

use super::messages::{InitConnection, TunnelMessage, 
    RelayListenIn, RelayListenOut, RejectConnection, IncomingConnection};
use super::types::{IncomingConn, IncomingConnInner, 
    IncomingListen, IncomingAccept, IncomingConnect};
use super::serialize::{deserialize_init_connection, deserialize_relay_listen_in,
                        serialize_relay_listen_out, serialize_tunnel_message,
                        deserialize_tunnel_message};
use super::listener::listener_keepalive;
use super::tunnel::tunnel_loop;



fn dispatch_conn<M,K,ME,KE>(receiver: M, sender: K, public_key: PublicKey, first_msg: Vec<u8>) 
    -> Option<IncomingConn<impl Stream<Item=RelayListenIn,Error=()>,
                              impl Sink<SinkItem=RelayListenOut,SinkError=()>,
                              impl Stream<Item=TunnelMessage,Error=()>,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()>,
                              impl Stream<Item=TunnelMessage,Error=()>,
                              impl Sink<SinkItem=TunnelMessage,SinkError=()>>>
where
    M: Stream<Item=Vec<u8>, Error=ME>,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE>,
{
    let sender = sender.sink_map_err(|_| ());
    let receiver = receiver.map_err(|_| ());
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


/// Process incoming connections
/// For each connection obtain the first message, and prepare the correct type according to this
/// first messages.
/// If waiting for the first message takes too long, discard the connection.
fn conn_processor<T,M,K,TE,ME,KE>(timer_client: TimerClient,
                    incoming_conns: T,
                    conn_timeout_ticks: usize) -> impl Stream<
                        Item=IncomingConn<impl Stream<Item=RelayListenIn,Error=()>,
                                          impl Sink<SinkItem=RelayListenOut,SinkError=()>,
                                          impl Stream<Item=TunnelMessage,Error=()>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>,
                                          impl Stream<Item=TunnelMessage,Error=()>,
                                          impl Sink<SinkItem=TunnelMessage,SinkError=()>>,
                        Error=()>
where
    T: Stream<Item=(M, K, PublicKey), Error=TE>,
    M: Stream<Item=Vec<u8>, Error=ME>,
    K: Sink<SinkItem=Vec<u8>, SinkError=KE>,
{
    let incoming_conns = incoming_conns.map_err(|_| ());
    let conn_timeout_ticks = usize_to_u64(conn_timeout_ticks).unwrap();
    let timer_streams = stream::iter_ok::<_, ()>(iter::repeat(()))
        .map_err(|_| ())
        .and_then(move |()| {
            timer_client.clone().request_timer_stream()
                .map_err(|_| ())
        });

    incoming_conns.zip(timer_streams)
    .and_then(move |((receiver, sender, public_key), timer_stream)| {
        let fut_receiver = 
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
            });

        let fut_time = timer_stream
            .take(conn_timeout_ticks)
            .for_each(|_| Ok(()))
            .map(|_| None);

        fut_receiver
            .select(fut_time)
            .map_err(|_| ())
    }).and_then(|(value, _last_future)| Ok(value))
    .filter_map(|opt_conn| opt_conn)
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

struct Listener<MT,KT> {
    half_tunnels: HashMap<PublicKey, HalfTunnel<MT,KT>>,
    tunnels: HashSet<PublicKey>,
    opt_sender: Option<mpsc::Sender<IncomingConnection>>,
}

impl<MT,KT> Listener<MT,KT> {
    fn new(sender: mpsc::Sender<IncomingConnection>) -> Self {
        Listener {
            half_tunnels: HashMap::new(),
            tunnels: HashSet::new(),
            opt_sender: Some(sender),
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
    ListeningNotInProgress,
    NoPendingHalfTunnel,
    AlreadyListening,
    EventReceiverError,
}


fn handle_accept<MT,KT,MA,KA,TCL,TS>(listeners: &mut HashMap<PublicKey, Listener<MT,KT>>,
                            acceptor_public_key: PublicKey,
                            incoming_accept: IncomingAccept<MA,KA>,
                            // TODO: This should be a oneshot:
                            tunnel_closed_sender: TCL,
                            timer_stream: TS,
                            keepalive_ticks: usize,
                            handle: &Handle) -> Result<(), RelayServerError>
where
    MT: Stream<Item=TunnelMessage,Error=()> + 'static,
    KT: Sink<SinkItem=TunnelMessage,SinkError=()> + 'static,
    MA: Stream<Item=TunnelMessage,Error=()> + 'static,
    KA: Sink<SinkItem=TunnelMessage,SinkError=()> + 'static,
    TCL: Sink<SinkItem=TunnelClosed, SinkError=()> + 'static,
    TS: Stream<Item=TimerTick,Error=()> + 'static,
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
                timer_stream,
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
        tunnel_closed_sender
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

    let (event_sender, event_receiver) = mpsc::channel::<RelayServerEvent<_,_,_,_,_,_>>(0);
    let event_sender = event_sender
        .sink_map_err(|_| ());
    let event_receiver = event_receiver
        .map_err(|_| RelayServerError::EventReceiverError);

    let relay_server_events = timer_stream
        .select(incoming_conns)
        .select(event_receiver);

    let mut incoming_conns_closed = false;
    let mut listeners: HashMap<PublicKey, Listener<_,_>> = HashMap::new();

    #[async]
    for relay_server_event in relay_server_events {
        let c_event_sender = event_sender.clone();
        let c_timer_client = timer_client.clone();
        match relay_server_event {
            RelayServerEvent::IncomingConn(incoming_conn) => {
                let IncomingConn {public_key, inner} = incoming_conn;
                match inner {
                    IncomingConnInner::Listen(incoming_listen) => {
                        if listeners.contains_key(&public_key) {
                            continue; // Discard Listen connection
                        }
                        let timer_stream = await!(c_timer_client.request_timer_stream())
                            .map_err(|_| RelayServerError::RequestTimerStreamError)?;
                        let (receiver, sender) = listener_keepalive(incoming_listen.receiver,
                                              incoming_listen.sender,
                                              timer_stream,
                                              keepalive_ticks,
                                              &handle);
                        
                        // Change the sender to be an mpsc::Sender, so that we can use the
                        // try_send() function.
                        let (mpsc_sender, mpsc_receiver) = mpsc::channel::<IncomingConnection>(0);
                        handle.spawn(
                            sender
                                .sink_map_err(|_| ())
                                .send_all(mpsc_receiver.map_err(|_| ()))
                                .then(|_| Ok(()))
                        );
                        let listener = Listener::new(mpsc_sender);
                        listeners.insert(public_key.clone(), listener);
                        let c_public_key = public_key.clone();
                        let receiver = receiver
                            .map_err(|_| ())
                            .map(move |reject_connection| RelayServerEvent::ListenerMessage(
                                    (c_public_key.clone(), reject_connection)))
                            .chain(stream::once(Ok(RelayServerEvent::ListenerClosed(public_key.clone()))));
                        handle.spawn(c_event_sender
                                     .sink_map_err(|_| ())
                                     .send_all(receiver)
                                     .then(|_| Ok(())));
                    },
                    IncomingConnInner::Accept(incoming_accept) => {
                        let tunnel_closed_sender = c_event_sender
                            .with(|tunnel_closed| Ok(RelayServerEvent::TunnelClosed(tunnel_closed)));
                        let timer_stream = await!(c_timer_client.request_timer_stream())
                            .map_err(|_| RelayServerError::RequestTimerStreamError)?;
                        let _ = handle_accept(&mut listeners,
                                      public_key.clone(),
                                      incoming_accept,
                                      tunnel_closed_sender,
                                      timer_stream,
                                      keepalive_ticks,
                                      &handle);
                    },
                    IncomingConnInner::Connect(incoming_connect) => {
                        let listener = match listeners.get_mut(&incoming_connect.connect_public_key) {
                            Some(listener) => listener,
                            None => continue, // Discard Connect connection
                        };
                        if listener.half_tunnels.contains_key(&public_key) || 
                            listener.tunnels.contains(&public_key) {
                            continue;
                        }

                        let half_tunnel = HalfTunnel {
                            conn_pair: ConnPair::new(incoming_connect.receiver, 
                                                     incoming_connect.sender),
                            ticks_to_close: keepalive_ticks,
                        };
                        if let Some(sender) = &mut listener.opt_sender {
                            // Try to send a message to listener about new pending connection:
                            if let Ok(()) = sender.try_send(IncomingConnection(public_key.clone())) {
                                listener.half_tunnels.insert(public_key.clone(), half_tunnel);
                            }
                        }
                    },
                }
            },
            RelayServerEvent::IncomingConnsClosed => incoming_conns_closed = true,
            RelayServerEvent::TunnelClosed(tunnel_closed) => {
                let listener = match listeners.get_mut(&tunnel_closed.listen_public_key) {
                    Some(listener) => listener,
                    None => continue,
                };
                listener.tunnels.remove(&tunnel_closed.init_public_key);
                if listener.opt_sender.is_none() && listener.tunnels.is_empty() {
                    listeners.remove(&tunnel_closed.listen_public_key);
                }
            },
            RelayServerEvent::ListenerMessage((public_key, RejectConnection(rejected_public_key))) => {
                let listener = match listeners.get_mut(&public_key) {
                    Some(listener) => listener,
                    None => continue,
                };
                let _ = listener.half_tunnels.remove(&rejected_public_key);
            },
            RelayServerEvent::ListenerClosed(public_key) => {
                let listener = match listeners.get_mut(&public_key) {
                    Some(listener) => listener,
                    None => continue,
                };
                listener.opt_sender = None;
                listener.half_tunnels = HashMap::new();
                if listener.tunnels.is_empty() {
                    listeners.remove(&public_key);
                }
            },
            RelayServerEvent::TimerTick => {
                // Remove old half tunnels:
                for (listener_public_key, listener) in &mut listeners {
                    listener.half_tunnels.retain(|init_public_key, half_tunnel| {
                        half_tunnel.ticks_to_close = half_tunnel.ticks_to_close.saturating_sub(1);
                        half_tunnel.ticks_to_close > 0
                    });
                }
            },
        }
        if incoming_conns_closed && listeners.is_empty() {
            break;
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    use futures::prelude::{async, await};
    use futures::sync::{mpsc, oneshot};
    use futures::Future;
    use tokio_core::reactor::Core;

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
    use timer::create_timer_incoming;
    use test::{receive, ReceiveError};

    use super::super::serialize::serialize_init_connection;

    #[test]
    fn test_dispatch_conn_basic() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let incoming_conn = dispatch_conn(receiver.map_err(|_| ()), 
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Listen(incoming_listen) => {},
            _ => panic!("Wrong IncomingConnInner"),
        };

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let accept_public_key = PublicKey::from(&[0x22; PUBLIC_KEY_LEN]);
        let first_msg = InitConnection::Accept(accept_public_key.clone());
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let incoming_conn = dispatch_conn(receiver.map_err(|_| ()), 
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Accept(incoming_accept) => 
                assert_eq!(incoming_accept.accept_public_key, accept_public_key),
            _ => panic!("Wrong IncomingConnInner"),
        };

        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let connect_public_key = PublicKey::from(&[0x33; PUBLIC_KEY_LEN]);
        let first_msg = InitConnection::Connect(connect_public_key.clone());
        let ser_first_msg = serialize_init_connection(&first_msg);
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let incoming_conn = dispatch_conn(receiver.map_err(|_| ()), 
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg).unwrap();
        assert_eq!(incoming_conn.public_key, public_key);
        match incoming_conn.inner {
            IncomingConnInner::Connect(incoming_connect) => 
                assert_eq!(incoming_connect.connect_public_key, connect_public_key),
            _ => panic!("Wrong IncomingConnInner"),
        };
    }

    #[test]
    fn test_dispatch_conn_invalid_first_msg() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let ser_first_msg = b"This is an invalid message".to_vec();
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let res = dispatch_conn(receiver.map_err(|_| ()), 
                                          sender.sink_map_err(|_| ()), 
                                          public_key.clone(), ser_first_msg);
        assert!(res.is_none());
    }


    /*
    #[async]
    fn first_message_sender(remote_sender: mpsc::Sender<Vec<u8>>, 
                            ser_first_msg: Vec<u8>) -> Result<(),()> {
        remote_sender.send(ser_first_msg)
        Ok(())
    }
    */

    #[test]
    fn test_conn_processor_basic() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let (local_sender, remote_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (remote_sender, local_receiver) = mpsc::channel::<Vec<u8>>(0);

        let incoming_conns = stream::iter_ok::<_, ()>(
            vec![(local_receiver, local_sender, public_key.clone())])
            .map_err(|_| ());

        let conn_timeout_ticks = 16;
        let processed_conns = conn_processor(timer_client, 
                       incoming_conns, 
                       conn_timeout_ticks);

        let first_msg = InitConnection::Listen;
        let ser_first_msg = serialize_init_connection(&first_msg);
        handle.spawn(remote_sender
                     .send(ser_first_msg)
                     .then(|res| {
                         match res {
                             Ok(_remote_sender) => Ok(()),
                             Err(_) => panic!("Sending first message failed!"),
                         }
                     })
        );
        // handle.spawn(first_message_sender(remote_sender, ser_first_msg));

        let (conn, processed_conns) =  core.run(receive(processed_conns)).unwrap();
        assert_eq!(conn.public_key, public_key);
        match conn.inner {
            IncomingConnInner::Listen(incoming_listen) => {},
            _ => panic!("Incorrect processed conn"),
        };

        let closed_error = core.run(receive(processed_conns));
        assert_eq!(closed_error.err().unwrap(), ReceiveError::Closed);
    }

    /*
    #[test]
    fn test_conn_processor_timeout() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();
    }
    */
}
