use std::collections::{HashMap, HashSet};
use std::fmt;
use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};

use common::futures_compat::send_to_sink;
use common::conn::{BoxStream, ConnPairVec};
use common::select_streams::{select_streams};

use timer::TimerClient;

use proto::crypto::PublicKey;
use proto::relay::messages::{IncomingConnection, RejectConnection};

use super::types::{IncomingAccept, IncomingConn, IncomingConnInner};

struct HalfTunnel {
    conn_pair: ConnPairVec,
    ticks_to_close: usize,
}

struct Listener {
    half_tunnels: HashMap<PublicKey, HalfTunnel>,
    tunnels: HashSet<PublicKey>,
    opt_sender: Option<mpsc::Sender<IncomingConnection>>,
}

impl Listener {
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

enum RelayServerEvent {
    IncomingConn(IncomingConn),
    IncomingConnsClosed,
    TunnelClosed(TunnelClosed),
    ListenerMessage((PublicKey, RejectConnection)),
    ListenerClosed(PublicKey),
    TimerTick,
    TimerClosed,
}

impl fmt::Debug for RelayServerEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RelayServerEvent::IncomingConn(_) => write!(f, "RelayServerEvent::IncomingConn"),
            RelayServerEvent::IncomingConnsClosed => {
                write!(f, "RelayServerEvent::IncomingConnsClosed")
            }
            RelayServerEvent::TunnelClosed(_) => write!(f, "RelayServerEvent::TunnelClosed"),
            RelayServerEvent::ListenerMessage(_) => write!(f, "RelayServerEvent::ListenerMessage"),
            RelayServerEvent::ListenerClosed(_) => write!(f, "RelayServerEvent::ListenerClosed"),
            RelayServerEvent::TimerTick => write!(f, "RelayServerEvent::TimerTick"),
            RelayServerEvent::TimerClosed => write!(f, "RelayServerEvent::TimerClosed"),
        }
    }
}

#[derive(Debug)]
pub enum RelayServerError {
    IncomingConnsError,
    RequestTimerStreamError,
    TimerStreamError,
    // TimerClosedError,
    ListeningNotInProgress,
    NoPendingHalfTunnel,
    AlreadyListening,
    EventReceiverError,
}

fn handle_accept<TCL>(
    listeners: &mut HashMap<PublicKey, Listener>,
    acceptor_public_key: PublicKey,
    incoming_accept: IncomingAccept,
    // TODO: This should be a oneshot:
    tunnel_closed_sender: TCL,
    spawner: impl Spawn,
) -> Result<(), RelayServerError>
where
    TCL: Sink<TunnelClosed, Error = ()> + Unpin + Send + 'static,
{
    let listener = match listeners.get_mut(&acceptor_public_key) {
        Some(listener) => listener,
        None => return Err(RelayServerError::ListeningNotInProgress),
    };
    let IncomingAccept {
        accept_public_key,
        conn_pair,
    } = incoming_accept;
    let (mut sender, receiver) = conn_pair.split();
    let conn_pair = match listener.half_tunnels.remove(&accept_public_key) {
        Some(HalfTunnel { conn_pair, .. }) => conn_pair,
        None => return Err(RelayServerError::NoPendingHalfTunnel),
    };
    let c_accept_public_key = accept_public_key.clone();

    let (mut remote_sender, remote_receiver) = conn_pair.split();

    let send_fut1 = async move {
        remote_sender
            .send_all(&mut receiver.map(Ok))
            .map_err(|e| error!("send_fut1 error: {:?}", e))
            .then(|_| future::ready(()))
            .await
    };
    let send_fut2 = async move {
        sender
            .send_all(&mut remote_receiver.map(Ok))
            .map_err(|e| error!("send_fut2 error: {:?}", e))
            .then(move |_| {
                let tunnel_closed = TunnelClosed {
                    init_public_key: c_accept_public_key,
                    listen_public_key: acceptor_public_key,
                };
                send_to_sink(tunnel_closed_sender, tunnel_closed).then(|_| future::ready(()))
            })
            .await
    };

    spawner.spawn(send_fut1).unwrap();
    spawner.spawn(send_fut2).unwrap();

    Ok(())
}

pub async fn relay_server_loop<S>(
    mut timer_client: TimerClient,
    incoming_conns: S,
    half_tunnel_ticks: usize,
    spawner: impl Spawn + Clone,
) -> Result<(), RelayServerError>
where
    S: Stream<Item = IncomingConn> + Unpin + Send,
{
    let timer_stream = timer_client
        .request_timer_stream()
        .await
        .map_err(|_| RelayServerError::RequestTimerStreamError)?;
    let timer_stream = timer_stream
        .map(|_| RelayServerEvent::TimerTick)
        .chain(stream::once(future::ready(RelayServerEvent::TimerClosed)));

    let incoming_conns = incoming_conns
        .map(RelayServerEvent::IncomingConn)
        .chain(stream::once(future::ready(
            RelayServerEvent::IncomingConnsClosed,
        )));

    let (event_sender, event_receiver) = mpsc::channel::<RelayServerEvent>(0);

    let mut relay_server_events = select_streams![timer_stream, incoming_conns, event_receiver];

    let mut incoming_conns_closed = false;
    let mut listeners: HashMap<PublicKey, Listener> = HashMap::new();

    while let Some(relay_server_event) = relay_server_events.next().await {
        let c_event_sender = event_sender.clone().sink_map_err(|_| ());
        match relay_server_event {
            RelayServerEvent::IncomingConn(incoming_conn) => {
                let IncomingConn { public_key, inner } = incoming_conn;
                match inner {
                    IncomingConnInner::Listen(incoming_listen) => {
                        if listeners.contains_key(&public_key) {
                            continue; // Discard Listen connection
                        }

                        let (sender, receiver) = incoming_listen.conn_pair.split();

                        // Change the sender to be an mpsc::Sender, so that we can use the
                        // try_send() function.
                        let (mpsc_sender, mpsc_receiver) =
                            mpsc::channel::<IncomingConnection>(0);
                        spawner
                            .spawn(async move {
                                let mut sender = sender.sink_map_err(|_| ());
                                sender
                                    .send_all(&mut mpsc_receiver.map(Ok))
                                    .then(|_| future::ready(()))
                                    .await
                            })
                            .unwrap();
                        let listener = Listener::new(mpsc_sender);
                        listeners.insert(public_key.clone(), listener);
                        let c_public_key = public_key.clone();
                        let receiver = receiver
                            .map(move |reject_connection| {
                                RelayServerEvent::ListenerMessage((
                                    c_public_key.clone(),
                                    reject_connection,
                                ))
                            })
                            .chain(stream::once(future::ready(
                                RelayServerEvent::ListenerClosed(public_key.clone()),
                            )));
                        spawner
                            .spawn(async move {
                                let mut c_event_sender = c_event_sender.sink_map_err(|_| ());
                                c_event_sender
                                    .send_all(&mut receiver.map(Ok))
                                    .then(|_| future::ready(()))
                                    .await
                            })
                            .unwrap();
                    }
                    IncomingConnInner::Accept(incoming_accept) => {
                        let tunnel_closed_sender = c_event_sender.with(|tunnel_closed| {
                            future::ready(Ok(RelayServerEvent::TunnelClosed(tunnel_closed)))
                        });
                        let _ = handle_accept(
                            &mut listeners,
                            public_key.clone(),
                            incoming_accept,
                            tunnel_closed_sender,
                            spawner.clone(),
                        )
                        .map_err(|e| warn!("handle_accept() error: {:?}", e));
                    }
                    IncomingConnInner::Connect(incoming_connect) => {
                        let listener = match listeners.get_mut(&incoming_connect.connect_public_key)
                        {
                            Some(listener) => listener,
                            None => continue, // Discard Connect connection
                        };
                        if listener.half_tunnels.contains_key(&public_key)
                            || listener.tunnels.contains(&public_key)
                        {
                            continue;
                        }

                        let half_tunnel = HalfTunnel {
                            conn_pair: incoming_connect.conn_pair,
                            ticks_to_close: half_tunnel_ticks,
                        };
                        if let Some(sender) = &mut listener.opt_sender {
                            // Try to send a message to listener about new pending connection:
                            if let Ok(()) = sender.try_send(IncomingConnection {
                                public_key: public_key.clone(),
                            }) {
                                listener
                                    .half_tunnels
                                    .insert(public_key.clone(), half_tunnel);
                            }
                        }
                    }
                }
            }
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
            }
            RelayServerEvent::ListenerMessage((
                public_key,
                RejectConnection {
                    public_key: rejected_public_key,
                },
            )) => {
                let listener = match listeners.get_mut(&public_key) {
                    Some(listener) => listener,
                    None => continue,
                };
                let _ = listener.half_tunnels.remove(&rejected_public_key);
            }
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
            }
            RelayServerEvent::TimerTick => {
                // Remove old half tunnels:
                for listener in listeners.values_mut() {
                    listener
                        .half_tunnels
                        .retain(|_init_public_key, half_tunnel| {
                            half_tunnel.ticks_to_close =
                                half_tunnel.ticks_to_close.saturating_sub(1);
                            half_tunnel.ticks_to_close > 0
                        });
                }
            }
            RelayServerEvent::TimerClosed => break,
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

    use futures::channel::mpsc;
    use futures::executor::{ThreadPool, LocalPool};
    use futures::task::{Spawn, SpawnExt};

    use crate::server::types::{IncomingAccept, IncomingConnect, IncomingListen};

    use common::conn::{ConnPair};

    use proto::crypto::PublicKey;
    use timer::create_timer_incoming;

    async fn task_relay_server_connect(
        spawner: impl Spawn + Clone + Send + 'static,
    ) -> Result<(), ()> {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (mut outgoing_conns, incoming_conns) = mpsc::channel::<_>(0);

        let half_tunnel_ticks: usize = 16;

        let fut_relay_server = relay_server_loop(
            timer_client,
            incoming_conns,
            half_tunnel_ticks,
            spawner.clone(),
        );

        spawner
            .spawn(
                fut_relay_server
                    .map_err(|_e| {
                        // println!("relay_server_loop() error: {:?}", e);
                        ()
                    })
                    .map(|_| ()),
            )
            .unwrap();

        /*      a          c          b
         * a_ca | <-- c_ca | c_cb --> | b_cb
         *      |          |          |
         * a_ac | --> c_ac | c_bc <-- | b_bc
         */

        let (a_ac, c_ac) = mpsc::channel::<RejectConnection>(0);
        let (c_ca, mut a_ca) = mpsc::channel::<IncomingConnection>(0);
        let (mut b_bc, c_bc) = mpsc::channel::<Vec<u8>>(0);
        let (c_cb, mut b_cb) = mpsc::channel::<Vec<u8>>(0);

        let a_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
        let b_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);

        let incoming_listen_a = IncomingListen {
            conn_pair: ConnPair::from_raw(c_ca.sink_map_err(|_| ()), c_ac),
        };
        let incoming_conn_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Listen(incoming_listen_a),
        };

        outgoing_conns.send(incoming_conn_a).await.unwrap();

        let incoming_connect_b = IncomingConnect {
            connect_public_key: a_public_key.clone(),
            conn_pair: ConnPairVec::from_raw(c_cb.sink_map_err(|_| ()), c_bc),
        };
        let incoming_conn_b = IncomingConn {
            public_key: b_public_key.clone(),
            inner: IncomingConnInner::Connect(incoming_connect_b),
        };

        outgoing_conns.send(incoming_conn_b).await.unwrap();

        let msg = a_ca.next().await.unwrap();
        assert_eq!(
            msg,
            IncomingConnection {
                public_key: b_public_key.clone()
            }
        );

        // Open a new connection to Accept:
        let (mut a_ac1, c_ac1) = mpsc::channel::<Vec<u8>>(0);
        let (c_ca1, mut a_ca1) = mpsc::channel::<Vec<u8>>(0);

        let incoming_accept_a = IncomingAccept {
            accept_public_key: b_public_key.clone(),
            conn_pair: ConnPairVec::from_raw(c_ca1.sink_map_err(|_| ()), c_ac1),
        };
        let incoming_conn_accept_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Accept(incoming_accept_a),
        };

        let _outgoing_conns = outgoing_conns.send(incoming_conn_accept_a).await.unwrap();

        a_ac1.send(vec![1, 2, 3]).await.unwrap();
        let msg = b_cb.next().await.unwrap();
        assert_eq!(msg, vec![1, 2, 3]);

        b_bc.send(vec![4, 3, 2, 1]).await.unwrap();
        let msg = a_ca1.next().await.unwrap();
        assert_eq!(msg, vec![4, 3, 2, 1]);

        // If one side's sender is dropped, the other side's receiver will be notified:
        drop(b_bc);
        assert!(a_ca1.next().await.is_none());

        // Drop here, to make sure values are not automatically dropped earlier:
        drop(a_ac);
        drop(a_ac1);
        drop(b_cb);
        Ok(())
    }

    #[test]
    fn test_relay_server_connect() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new()
            .run_until(task_relay_server_connect(thread_pool.clone()))
            .unwrap();
    }

    async fn task_relay_server_reject(
        spawner: impl Spawn + Clone + Send + 'static,
    ) -> Result<(), ()> {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (mut outgoing_conns, incoming_conns) = mpsc::channel::<_>(0);

        let half_tunnel_ticks: usize = 16;

        let fut_relay_server = relay_server_loop(
            timer_client,
            incoming_conns,
            half_tunnel_ticks,
            spawner.clone(),
        );

        spawner
            .spawn(
                fut_relay_server
                    .map_err(|_e| {
                        // println!("relay_server_loop() error: {:?}", e);
                        ()
                    })
                    .map(|_| ()),
            )
            .unwrap();

        /*      a          c          b
         * a_ca | <-- c_ca | c_cb --> | b_cb
         *      |          |          |
         * a_ac | --> c_ac | c_bc <-- | b_bc
         */

        let (mut a_ac, c_ac) = mpsc::channel::<RejectConnection>(0);
        let (c_ca, mut a_ca) = mpsc::channel::<IncomingConnection>(0);
        let (b_bc, c_bc) = mpsc::channel::<Vec<u8>>(0);
        let (c_cb, mut b_cb) = mpsc::channel::<Vec<u8>>(0);

        let a_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
        let b_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);

        let incoming_listen_a = IncomingListen {
            conn_pair: ConnPair::from_raw(c_ca , c_ac)
        };
        let incoming_conn_a = IncomingConn {
            public_key: a_public_key.clone(),
            inner: IncomingConnInner::Listen(incoming_listen_a),
        };

        outgoing_conns.send(incoming_conn_a).await.unwrap();

        let incoming_connect_b = IncomingConnect {
            connect_public_key: a_public_key.clone(),
            conn_pair: ConnPairVec::from_raw(c_cb , c_bc),
        };
        let incoming_conn_b = IncomingConn {
            public_key: b_public_key.clone(),
            inner: IncomingConnInner::Connect(incoming_connect_b),
        };

        outgoing_conns.send(incoming_conn_b).await.unwrap();

        let msg = a_ca.next().await.unwrap();
        assert_eq!(
            msg,
            IncomingConnection {
                public_key: b_public_key.clone()
            }
        );

        // This is done to help the compiler deduce the types for
        // IncomingConn:
        if false {
            // Open a new connection to Accept:
            let (_a_ac1, c_ac1) = mpsc::channel::<Vec<u8>>(0);
            let (c_ca1, _a_ca1) = mpsc::channel::<Vec<u8>>(0);

            let incoming_accept_a = IncomingAccept {
                accept_public_key: b_public_key.clone(),
                conn_pair: ConnPairVec::from_raw(c_ca1, c_ac1),
            };
            let incoming_conn_accept_a = IncomingConn {
                public_key: a_public_key.clone(),
                inner: IncomingConnInner::Accept(incoming_accept_a),
            };
            outgoing_conns.send(incoming_conn_accept_a).await.unwrap();
        }

        // A rejects B's connection:
        let reject_connection = RejectConnection {
            public_key: b_public_key,
        };
        a_ac.send(reject_connection).await.unwrap();

        // B should be notified that the connection is closed:
        assert!(b_cb.next().await.is_none());

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
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new()
            .run_until(task_relay_server_reject(thread_pool.clone()))
            .unwrap();
    }

    // TODO: Add tests:
    // - Timeout of half tunnels
    //      (Do some action first, to make sure timer_stream was already obtained).
    // - Graceful shutdown if incoming_conns is closed.
    // - Duplicate connections should be denied. (Same (initiator_pk, listener_pk) pair).
    // - Tunnel keeps working even if listener is disconnected.
}
