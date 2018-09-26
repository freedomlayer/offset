#![allow(unused)]
use std::collections::HashMap;
use bytes::Bytes;
use futures::{stream, Stream, Sink};
use futures::prelude::{async, await};
use crypto::identity::PublicKey;
use timer::TimerClient;

use self::messages::{TunnelMessage, RelayListenIn, RelayListenOut};

mod messages;


struct ConnPair<T,W> {
    receiver: Box<Stream<Item=T, Error=()>>,
    sender: Box<Sink<SinkItem=W, SinkError=()>>,
}

impl<T,W> ConnPair<T,W> {
    fn new<M,K>(receiver: M, sender: K) -> ConnPair<T,W> 
    where
        M: Stream<Item=T, Error=()> + 'static,
        K: Sink<SinkItem=W, SinkError=()> + 'static,
    {
        ConnPair {
            receiver: Box::new(receiver),
            sender: Box::new(sender),
        }
    }

    fn into_inner(self) -> (Box<Stream<Item=T, Error=()>>,
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

enum TunnelError {
    RequestTimerStream,
    TimerStream,
    TunnelReceiver,
    Sender,
    Timeout,
    TimerClosed,
}

pub enum TunnelEvent {
    Receiver1(TunnelMessage),
    Receiver2(TunnelMessage),
    Receiver1Closed,
    Receiver2Closed,
    TimerTick,
}


#[async]
fn tunnel_loop(conn_pair1: ConnPair<TunnelMessage, TunnelMessage>, 
               conn_pair2: ConnPair<TunnelMessage, TunnelMessage>, 
               timer_client: TimerClient, keepalive_ticks: usize) -> Result<(), TunnelError> {

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| TunnelError::RequestTimerStream)?;
    let timer_stream = timer_stream.map(|_| TunnelEvent::TimerTick)
        .map_err(|_| TunnelError::TimerStream)
        .chain(stream::once(Err(TunnelError::TimerClosed)));

    let (receiver1, mut sender1) = conn_pair1.into_inner();
    let (receiver2, mut sender2) = conn_pair2.into_inner();

    let mut open_receivers: usize = 2;
    // Ticks left until we drop the connection due to inactivity.
    let mut ticks_to_timeout1: usize = keepalive_ticks;
    let mut ticks_to_timeout2: usize = keepalive_ticks;
    // Ticks left until we need to send a keepalive to remote side:
    let mut ticks_to_keepalive1: usize = keepalive_ticks / 2;
    let mut ticks_to_keepalive2: usize = keepalive_ticks / 2;

    let receiver1 = receiver1.map(TunnelEvent::Receiver1)
        .map_err(|_| TunnelError::TunnelReceiver)
        .chain(stream::once(Ok(TunnelEvent::Receiver1Closed)));
    let receiver2 = receiver2.map(TunnelEvent::Receiver2)
        .map_err(|_| TunnelError::TunnelReceiver)
        .chain(stream::once(Ok(TunnelEvent::Receiver2Closed)));

    let tunnel_events = timer_stream.select(receiver1).select(receiver2);

    #[async]
    for tunnel_event in tunnel_events {
        match tunnel_event {
            TunnelEvent::Receiver1(tun_msg) => {
                match tun_msg {
                    TunnelMessage::KeepAlive => ticks_to_timeout1 = keepalive_ticks,
                    TunnelMessage::Message(msg) => {
                        sender1 = await!(sender1.send(TunnelMessage::Message(msg)))
                            .map_err(|_| TunnelError::Sender)?;
                        ticks_to_keepalive1 = keepalive_ticks / 2;
                    },
                };
            },
            TunnelEvent::Receiver2(tun_msg) => {
                match tun_msg {
                    TunnelMessage::KeepAlive => ticks_to_timeout2 = keepalive_ticks,
                    TunnelMessage::Message(msg) => {
                        sender2 = await!(sender2.send(TunnelMessage::Message(msg)))
                            .map_err(|_| TunnelError::Sender)?;
                        ticks_to_keepalive2 = keepalive_ticks / 2;
                    },
                };
            },
            TunnelEvent::Receiver1Closed | TunnelEvent::Receiver2Closed => {
                open_receivers = open_receivers.saturating_sub(1);
                if open_receivers == 0 {
                    return Ok(())
                }
            },
            TunnelEvent::TimerTick => {
                ticks_to_timeout1 = ticks_to_timeout1.saturating_sub(1);
                ticks_to_timeout2 = ticks_to_timeout2.saturating_sub(1);
                if ticks_to_timeout1 == 0 || ticks_to_timeout2 == 0 {
                    return Err(TunnelError::Timeout)
                }
                ticks_to_keepalive1 = ticks_to_keepalive1.saturating_sub(1);
                if ticks_to_keepalive1 == 0 {
                    sender1 = await!(sender1.send(TunnelMessage::KeepAlive))
                        .map_err(|_| TunnelError::Sender)?;
                    ticks_to_keepalive1 = keepalive_ticks / 2;
                }

                ticks_to_keepalive2 = ticks_to_keepalive2.saturating_sub(1);
                if ticks_to_keepalive2 == 0 {
                    sender2 = await!(sender2.send(TunnelMessage::KeepAlive))
                        .map_err(|_| TunnelError::Sender)?;
                    ticks_to_keepalive2 = keepalive_ticks / 2;
                }
            },
        }
    }
    unreachable!();
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
