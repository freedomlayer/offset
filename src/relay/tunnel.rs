use timer::TimerClient;
use futures::prelude::{async, await};
use futures::{Stream, stream, Sink};
use super::messages::TunnelMessage;

#[derive(Debug)]
pub enum TunnelError {
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
fn tunnel_loop<R1,S1,R2,S2>(receiver1: R1, mut sender1: S1,
                            receiver2: R2, mut sender2: S2,
                            timer_client: TimerClient, 
                            keepalive_ticks: usize) -> Result<(), TunnelError> 
where
    R1: Stream<Item=TunnelMessage, Error=()> + 'static,
    S1: Sink<SinkItem=TunnelMessage, SinkError=()> + 'static,
    R2: Stream<Item=TunnelMessage, Error=()> + 'static,
    S2: Sink<SinkItem=TunnelMessage, SinkError=()> + 'static,
{

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| TunnelError::RequestTimerStream)?;
    let timer_stream = timer_stream.map(|_| TunnelEvent::TimerTick)
        .map_err(|_| TunnelError::TimerStream)
        .chain(stream::once(Err(TunnelError::TimerClosed)));

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

