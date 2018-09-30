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
pub fn tunnel_loop<R1,S1,R2,S2,ER1,ES1,ER2,ES2>(receiver1: R1, mut sender1: S1,
                            receiver2: R2, mut sender2: S2,
                            timer_client: TimerClient, 
                            keepalive_ticks: usize) -> Result<(), TunnelError> 
where
    R1: Stream<Item=TunnelMessage, Error=ER1> + 'static,
    S1: Sink<SinkItem=TunnelMessage, SinkError=ES1> + 'static,
    R2: Stream<Item=TunnelMessage, Error=ER2> + 'static,
    S2: Sink<SinkItem=TunnelMessage, SinkError=ES2> + 'static,
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
                        sender2 = await!(sender2.send(TunnelMessage::Message(msg)))
                            .map_err(|_| TunnelError::Sender)?;
                        ticks_to_keepalive1 = keepalive_ticks / 2;
                    },
                };
            },
            TunnelEvent::Receiver2(tun_msg) => {
                match tun_msg {
                    TunnelMessage::KeepAlive => ticks_to_timeout2 = keepalive_ticks,
                    TunnelMessage::Message(msg) => {
                        sender1 = await!(sender1.send(TunnelMessage::Message(msg)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::prelude::{async, await};
    use futures::sync::{mpsc, oneshot};
    use futures::Future;
    use tokio_core::reactor::Core;
    use timer::create_timer_incoming;

    #[derive(Debug, Eq, PartialEq)]
    enum ReadError {
        Closed,
        Error,
    }

    #[async]
    fn receive<T, EM, M: 'static>(reader: M) -> Result<(T, M), ReadError>
        where M: Stream<Item=T, Error=EM>,
    {
        match await!(reader.into_future()) {
            Ok((opt_reader_message, ret_reader)) => {
                match opt_reader_message {
                    Some(reader_message) => Ok((reader_message, ret_reader)),
                    None => return Err(ReadError::Closed),
                }
            },
            Err(_) => return Err(ReadError::Error),
        }
    }

    #[async]
    fn run_a(mut receiver: mpsc::Receiver<TunnelMessage>, 
             mut sender: mpsc::Sender<TunnelMessage>,
             mut tick_sender: mpsc::Sender<()>,
             fin_sender: oneshot::Sender<bool>) -> Result<(), ()> {

        sender = await!(sender.send(TunnelMessage::KeepAlive)).unwrap();
        sender = await!(sender.send(TunnelMessage::KeepAlive)).unwrap();
        sender = await!(sender.send(TunnelMessage::Message(vec![1,2,3,4]))).unwrap();
        let (data, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(data, TunnelMessage::Message(vec![5,6,7]));

        for _ in 0 .. 8usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }
        let (data, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(data, TunnelMessage::KeepAlive);

        for _ in 0 .. 8usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }
        let err = match await!(receive(receiver)) {
            Ok(_) => unreachable!(),
            Err(e) => e
        };
        assert_eq!(err, ReadError::Closed);

        fin_sender.send(true).unwrap();
        Ok(())
    }

    #[async]
    fn run_b(mut receiver: mpsc::Receiver<TunnelMessage>, 
             mut sender: mpsc::Sender<TunnelMessage>,
             _tick_sender: mpsc::Sender<()>,
             fin_sender: oneshot::Sender<bool>) -> Result<(), ()> {

        let (data, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(data, TunnelMessage::Message(vec![1,2,3,4]));
        sender = await!(sender.send(TunnelMessage::KeepAlive)).unwrap();
        sender = await!(sender.send(TunnelMessage::Message(vec![5,6,7]))).unwrap();

        fin_sender.send(true).unwrap();
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

        let tloop = tunnel_loop(c_ac, c_ca, 
                                c_bc, c_cb,
                                timer_client,
                                keepalive_ticks);
        handle.spawn(tloop.then(|e| {
            println!("tloop error occured: {:?}", e);
            Ok(())
        }));

        let (fin_sender_a, fin_receiver_a) = oneshot::channel::<bool>();
        let (fin_sender_b, fin_receiver_b) = oneshot::channel::<bool>();

        handle.spawn(run_a(a_ca, a_ac, tick_sender.clone(), fin_sender_a));
        handle.spawn(run_b(b_cb, b_bc, tick_sender.clone(), fin_sender_b));

        assert_eq!(core.run(fin_receiver_a).unwrap(), true);
        assert_eq!(core.run(fin_receiver_b).unwrap(), true);

    }
}

