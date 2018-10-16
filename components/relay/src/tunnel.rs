use std::marker::Unpin;
use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use timer::TimerTick;
use proto::relay::messages::TunnelMessage;

#[derive(Debug)]
pub enum TunnelError {
    RequestTimerStream,
    TimerStream,
    TunnelReceiver,
    Sender,
    Timeout,
    TimerClosed,
}

#[derive(Debug)]
pub enum TunnelEvent {
    Receiver1(TunnelMessage),
    Receiver2(TunnelMessage),
    Receiver1Closed,
    Receiver2Closed,
    TimerTick,
    TimerClosed,
}

pub async fn tunnel_loop<R1,S1,R2,S2,ES1,ES2,TS>(receiver1: R1, mut sender1: S1,
                            receiver2: R2, mut sender2: S2,
                            timer_stream: TS, 
                            keepalive_ticks: usize) -> Result<(), TunnelError> 
where
    R1: Stream<Item=TunnelMessage> + Unpin + 'static,
    S1: Sink<SinkItem=TunnelMessage, SinkError=ES1> + Unpin + 'static,
    R2: Stream<Item=TunnelMessage> + Unpin + 'static,
    S2: Sink<SinkItem=TunnelMessage, SinkError=ES2> + Unpin + 'static,
    TS: Stream<Item=TimerTick> + Unpin + 'static,
{

    let timer_stream = timer_stream.map(|_| TunnelEvent::TimerTick)
        .chain(stream::once(future::ready(TunnelEvent::TimerClosed)));

    // Ticks left until we drop the connection due to inactivity.
    let mut ticks_to_timeout1: usize = keepalive_ticks;
    let mut ticks_to_timeout2: usize = keepalive_ticks;
    // Ticks left until we need to send a keepalive to remote side:
    let mut ticks_to_keepalive1: usize = keepalive_ticks / 2;
    let mut ticks_to_keepalive2: usize = keepalive_ticks / 2;

    let receiver1 = receiver1.map(TunnelEvent::Receiver1)
        .chain(stream::once(future::ready(TunnelEvent::Receiver1Closed)));
    let receiver2 = receiver2.map(TunnelEvent::Receiver2)
        .chain(stream::once(future::ready(TunnelEvent::Receiver2Closed)));

    let mut tunnel_events = timer_stream.select(receiver1).select(receiver2);

    while let Some(tunnel_event) = await!(tunnel_events.next()) {
        match tunnel_event {
            TunnelEvent::Receiver1(tun_msg) => {
                match tun_msg {
                    TunnelMessage::KeepAlive => ticks_to_timeout1 = keepalive_ticks,
                    TunnelMessage::Message(msg) => {
                        await!(sender2.send(TunnelMessage::Message(msg)))
                            .map_err(|_| TunnelError::Sender)?;
                        ticks_to_keepalive1 = keepalive_ticks / 2;
                    },
                };
            },
            TunnelEvent::Receiver2(tun_msg) => {
                match tun_msg {
                    TunnelMessage::KeepAlive => ticks_to_timeout2 = keepalive_ticks,
                    TunnelMessage::Message(msg) => {
                        await!(sender1.send(TunnelMessage::Message(msg)))
                            .map_err(|_| TunnelError::Sender)?;
                        ticks_to_keepalive2 = keepalive_ticks / 2;
                    },
                };
            },
            TunnelEvent::Receiver1Closed | TunnelEvent::Receiver2Closed => return Ok(()),
            TunnelEvent::TimerTick => {
                ticks_to_timeout1 = ticks_to_timeout1.saturating_sub(1);
                ticks_to_timeout2 = ticks_to_timeout2.saturating_sub(1);
                if ticks_to_timeout1 == 0 || ticks_to_timeout2 == 0 {
                    return Err(TunnelError::Timeout)
                }
                ticks_to_keepalive1 = ticks_to_keepalive1.saturating_sub(1);
                if ticks_to_keepalive1 == 0 {
                    await!(sender1.send(TunnelMessage::KeepAlive))
                        .map_err(|_| TunnelError::Sender)?;
                    ticks_to_keepalive1 = keepalive_ticks / 2;
                }

                ticks_to_keepalive2 = ticks_to_keepalive2.saturating_sub(1);
                if ticks_to_keepalive2 == 0 {
                    await!(sender2.send(TunnelMessage::KeepAlive))
                        .map_err(|_| TunnelError::Sender)?;
                    ticks_to_keepalive2 = keepalive_ticks / 2;
                }
            },
            TunnelEvent::TimerClosed => return Err(TunnelError::TimerClosed),
        }
    }
    unreachable!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::sync::mpsc;
    use futures::Future;
    use tokio_core::reactor::Core;
    use timer::create_timer_incoming;
    use utils::async_test_utils::{receive, ReceiveError};

    async fn run_tunnel_basic(mut receiver_a: mpsc::Receiver<TunnelMessage>, 
                     mut sender_a: mpsc::Sender<TunnelMessage>,
                     mut receiver_b: mpsc::Receiver<TunnelMessage>, 
                     mut sender_b:  mpsc::Sender<TunnelMessage>,
                     mut tick_sender: mpsc::Sender<()>) 
        -> Result<(), ()> {

        // Send and receive messages:
        // (KeepAlive messages are not passed to the other side)
        sender_a = await!(sender_a.send(TunnelMessage::KeepAlive)).unwrap();
        sender_a = await!(sender_a.send(TunnelMessage::KeepAlive)).unwrap();

        sender_a = await!(sender_a.send(TunnelMessage::Message(vec![1,2,3,4]))).unwrap();
        let (msg, new_receiver_b) = await!(receive(receiver_b)).unwrap();
        receiver_b = new_receiver_b;
        assert_eq!(msg, TunnelMessage::Message(vec![1,2,3,4]));
        sender_b = await!(sender_b.send(TunnelMessage::KeepAlive)).unwrap();

        sender_b = await!(sender_b.send(TunnelMessage::Message(vec![5,6,7]))).unwrap();
        let (msg, new_receiver_a) = await!(receive(receiver_a)).unwrap();
        receiver_a = new_receiver_a;
        assert_eq!(msg, TunnelMessage::Message(vec![5,6,7]));

        // We should get keepalive after some time passes:
        for _ in 0 .. 8usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }
        let (msg, new_receiver_a) = await!(receive(receiver_a)).unwrap();
        receiver_a = new_receiver_a;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        let (msg, new_receiver_b) = await!(receive(receiver_b)).unwrap();
        receiver_b = new_receiver_b;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        for _ in 0 .. 7usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }
        // Send a keepalive, so that we won't get disconnected:
        sender_a = await!(sender_a.send(TunnelMessage::KeepAlive)).unwrap();
        sender_b = await!(sender_b.send(TunnelMessage::KeepAlive)).unwrap();

        // Wait one tick:
        tick_sender = await!(tick_sender.send(())).unwrap();

        let (msg, new_receiver_a) = await!(receive(receiver_a)).unwrap();
        receiver_a = new_receiver_a;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        let (msg, new_receiver_b) = await!(receive(receiver_b)).unwrap();
        receiver_b = new_receiver_b;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        // Send a message from A to B. 
        // A will be disconnected 16 ticks from now,
        // B will be disconnected 15 ticks from now.
        // A will get a keepalive 8 ticks from now.
        // B will get a keepalive 8 ticks from now.
        sender_a = await!(sender_a.send(TunnelMessage::Message(vec![8,8,8]))).unwrap();
        let (msg, new_receiver_b) = await!(receive(receiver_b)).unwrap();
        receiver_b = new_receiver_b;
        assert_eq!(msg, TunnelMessage::Message(vec![8,8,8]));

        for _ in 0 .. 8usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }

        let (msg, new_receiver_b) = await!(receive(receiver_b)).unwrap();
        receiver_b = new_receiver_b;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        let (msg, new_receiver_a) = await!(receive(receiver_a)).unwrap();
        receiver_a = new_receiver_a;
        assert_eq!(msg, TunnelMessage::KeepAlive);

        for _ in 0 .. 7usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }

        // B timeouts here. As a result the whole tunnel is closed:
        let err = match await!(receive(receiver_b)) {
            Ok(_) => unreachable!(),
            Err(e) => e
        };
        assert_eq!(err, ReceiveError::Closed);

        let err = match await!(receive(receiver_a)) {
            Ok(_) => unreachable!(),
            Err(e) => e
        };
        assert_eq!(err, ReceiveError::Closed);

        Ok(())

    }

    #[test]
    fn test_tunnel_basic() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        /*      a          c          b
         * a_ca | <-- c_ca | c_cb --> | b_cb
         *      |          |          |
         * a_ac | --> c_ac | c_bc <-- | b_bc
        */

        let (a_ac, c_ac) = mpsc::channel::<TunnelMessage>(0);
        let (c_ca, a_ca) = mpsc::channel::<TunnelMessage>(0);
        let (b_bc, c_bc) = mpsc::channel::<TunnelMessage>(0);
        let (c_cb, b_cb) = mpsc::channel::<TunnelMessage>(0);

        let keepalive_ticks = 16;
        let timer_stream = core.run(timer_client.request_timer_stream()).unwrap();

        let tloop = tunnel_loop(c_ac, c_ca, 
                                c_bc, c_cb,
                                timer_stream,
                                keepalive_ticks);
        handle.spawn(tloop.then(|e| {
            println!("tloop error occured: {:?}", e);
            Ok(())
        }));


        core.run(
            run_tunnel_basic(a_ca,a_ac,
                             b_cb,b_bc,
                             tick_sender)
        ).unwrap();
    }
}

