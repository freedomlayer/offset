use std::collections::{HashMap, HashSet};
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use futures::prelude::{async, await}; 
use tokio_core::reactor::Handle;

use timer::{TimerTick, TimerClient};
use crypto::identity::PublicKey;

use super::messages::{TunnelMessage, RelayListenIn, 
    RelayListenOut, RejectConnection, IncomingConnection};

use super::types::{IncomingConn, IncomingConnInner, 
    IncomingAccept};

use super::listener::listener_keepalive;
use super::tunnel::tunnel_loop;

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

#[derive(Debug)]
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
                for (_listener_public_key, listener) in &mut listeners {
                    listener.half_tunnels.retain(|_init_public_key, half_tunnel| {
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
    use futures::sync::{mpsc};
    use futures::Future;
    use tokio_core::reactor::{Core, Handle};

    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
    use timer::create_timer_incoming;
    use test::{receive, ReceiveError};
    use super::super::types::{IncomingListen, 
        IncomingConnect, IncomingAccept};

    #[async]
    fn task_relay_server_connect(handle: Handle) -> Result<(),()> {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        let (outgoing_conns, incoming_conns) = mpsc::channel::<_>(0);

        let keepalive_ticks: usize = 16;

        let fut_relay_server = relay_server(timer_client,
                     incoming_conns,
                     keepalive_ticks,
                     handle.clone());

        handle.spawn(
            fut_relay_server
                .map_err(|e| {
                    println!("relay_server() error: {:?}", e);
                    ()
                })
        );

        /*      a          c          b
         * a_ca | <-- c_ca | c_cb --> | b_cb
         *      |          |          |
         * a_ac | --> c_ac | c_bc <-- | b_bc
        */

        let (a_ac, c_ac) = mpsc::channel::<RelayListenIn>(0);
        let (c_ca, a_ca) = mpsc::channel::<RelayListenOut>(0);
        let (mut b_bc, c_bc) = mpsc::channel::<TunnelMessage>(0);
        let (c_cb, mut b_cb) = mpsc::channel::<TunnelMessage>(0);

        let a_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let b_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);

        let incoming_listen_a = IncomingListen {
            receiver: c_ac.map_err(|_| ()),
            sender: c_ca.sink_map_err(|_| ()),
        };
        let incoming_conn_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Listen(incoming_listen_a),
        };

        let outgoing_conns = await!(outgoing_conns.send(incoming_conn_a)).unwrap();

        let incoming_connect_b = IncomingConnect {
            receiver: c_bc.map_err(|_| ()),
            sender: c_cb.sink_map_err(|_| ()),
            connect_public_key: a_public_key.clone(),
        };
        let incoming_conn_b = IncomingConn {
            public_key: b_public_key.clone(),
            inner: IncomingConnInner::Connect(incoming_connect_b),
        };

        let outgoing_conns = await!(outgoing_conns.send(incoming_conn_b)).unwrap();

        let (msg, _new_a_ca) = await!(receive(a_ca)).unwrap();
        assert_eq!(msg, RelayListenOut::IncomingConnection(
                IncomingConnection(b_public_key.clone())));

        // Open a new connection to Accept:
        let (mut a_ac1, c_ac1) = mpsc::channel::<TunnelMessage>(0);
        let (c_ca1, mut a_ca1) = mpsc::channel::<TunnelMessage>(0);

        let incoming_accept_a = IncomingAccept {
            receiver: c_ac1.map_err(|_| ()),
            sender: c_ca1.sink_map_err(|_| ()),
            accept_public_key: b_public_key.clone(),
        };
        let incoming_conn_accept_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Accept(incoming_accept_a),
        };

        let _outgoing_conns = await!(outgoing_conns.send(incoming_conn_accept_a)).unwrap();

        a_ac1 = await!(a_ac1.send(TunnelMessage::Message(vec![1,2,3]))).unwrap();
        let (msg, new_b_cb) = await!(receive(b_cb)).unwrap();
        b_cb = new_b_cb;
        assert_eq!(msg, TunnelMessage::Message(vec![1,2,3]));

        b_bc = await!(b_bc.send(TunnelMessage::Message(vec![4,3,2,1]))).unwrap();
        let (msg, new_a_ca1) = await!(receive(a_ca1)).unwrap();
        a_ca1 = new_a_ca1;
        assert_eq!(msg, TunnelMessage::Message(vec![4,3,2,1]));

        // If one side's sender is dropped, the other side's receiver will be notified:
        drop(b_bc);
        assert_eq!(ReceiveError::Closed, await!(receive(a_ca1)).err().unwrap());

        // Drop here, to make sure values are not automatically dropped earlier:
        drop(a_ac);
        drop(a_ac1);
        drop(b_cb);
        Ok(())
    }


    #[test]
    fn test_relay_server_connect() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(task_relay_server_connect(handle)).unwrap();

    }

    
    #[async]
    fn task_relay_server_reject(handle: Handle) -> Result<(),()> {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        let (mut outgoing_conns, incoming_conns) = mpsc::channel::<_>(0);

        let keepalive_ticks: usize = 16;

        let fut_relay_server = relay_server(timer_client,
                     incoming_conns,
                     keepalive_ticks,
                     handle.clone());

        handle.spawn(
            fut_relay_server
                .map_err(|e| {
                    println!("relay_server() error: {:?}", e);
                    ()
                })
        );

        /*      a          c          b
         * a_ca | <-- c_ca | c_cb --> | b_cb
         *      |          |          |
         * a_ac | --> c_ac | c_bc <-- | b_bc
        */

        let (mut a_ac, c_ac) = mpsc::channel::<RelayListenIn>(0);
        let (c_ca, mut a_ca) = mpsc::channel::<RelayListenOut>(0);
        let (b_bc, c_bc) = mpsc::channel::<TunnelMessage>(0);
        let (c_cb, b_cb) = mpsc::channel::<TunnelMessage>(0);

        let a_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let b_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);

        let incoming_listen_a = IncomingListen {
            receiver: c_ac.map_err(|_| ()),
            sender: c_ca.sink_map_err(|_| ()),
        };
        let incoming_conn_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Listen(incoming_listen_a),
        };

        outgoing_conns = await!(outgoing_conns.send(incoming_conn_a)).unwrap();

        let incoming_connect_b = IncomingConnect {
            receiver: c_bc.map_err(|_| ()),
            sender: c_cb.sink_map_err(|_| ()),
            connect_public_key: a_public_key.clone(),
        };
        let incoming_conn_b = IncomingConn {
            public_key: b_public_key.clone(),
            inner: IncomingConnInner::Connect(incoming_connect_b),
        };

        outgoing_conns = await!(outgoing_conns.send(incoming_conn_b)).unwrap();

        let (msg, new_a_ca) = await!(receive(a_ca)).unwrap();
        a_ca = new_a_ca;
        assert_eq!(msg, RelayListenOut::IncomingConnection(
                IncomingConnection(b_public_key.clone())));

        // This is done to help the compiler deduce the types for 
        // IncomingConn:
        if false {
            // Open a new connection to Accept:
            let (_a_ac1, c_ac1) = mpsc::channel::<TunnelMessage>(0);
            let (c_ca1, _a_ca1) = mpsc::channel::<TunnelMessage>(0);

            let incoming_accept_a = IncomingAccept {
                receiver: c_ac1.map_err(|_| ()),
                sender: c_ca1.sink_map_err(|_| ()),
                accept_public_key: b_public_key.clone(),
            };
            let incoming_conn_accept_a = IncomingConn {
                public_key: a_public_key.clone(),
                inner: IncomingConnInner::Accept(incoming_accept_a),
            };
            outgoing_conns = await!(outgoing_conns.send(incoming_conn_accept_a)).unwrap();
        }

        // A rejects B's connection:
        let reject_connection = RelayListenIn::RejectConnection(
            RejectConnection(b_public_key));
        a_ac = await!(a_ac.send(reject_connection)).unwrap();

        // B should be notified that the connection is closed:
        assert_eq!(ReceiveError::Closed, await!(receive(b_cb)).err().unwrap());

        // Drop here, to make sure values are not automatically dropped earlier:
        drop(a_ac);
        drop(a_ca);
        // drop(b_cb);
        drop(b_bc);
        drop(outgoing_conns);
        Ok(())
    }


    #[test]
    fn test_relay_server_reject() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(task_relay_server_reject(handle)).unwrap();

    }


    // TODO: Add tests:
    // - Timeout of half tunnels
    //      (Do some action first, to make sure timer_stream was already obtained).
    // - Graceful shutdown if incoming_conns is closed.
    // - Duplicate connections should be denied. (Same (initiator_pk, listener_pk) pair).
    // - Tunnel keeps working even if listener is disconnected.
}
