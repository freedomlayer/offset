#![allow(unused)]
use std::marker::Unpin;
use futures::{future, Future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use timer::TimerTick;
use proto::relay::messages::{RejectConnection, IncomingConnection};

#[derive(Debug)]
enum ListenerLoopError {
    RequestTimerStreamError,
    TimerClosed,
    OutgoingMessages,
    ListenerReceiver,
    TimerStream,
    ListenerSenderFailure,
    IncomingMessagesFailure,
}

#[derive(Debug, Clone)]
enum ListenerEvent {
    ListenerReceiver(RejectConnection),
    OutgoingMessage(IncomingConnection),
    ListenerReceiverClosed,
    OutgoingMessagesClosed,
    TimerTick,
    TimerClosed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::{mpsc, oneshot};
    use futures::Future;
    use futures::executor::ThreadPool;
    use timer::create_timer_incoming;
    use utils::async_test_utils::{receive, ReceiveError};
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    async fn run_listener_keepalive_basic(
        mut wreceiver: mpsc::Receiver<RejectConnection>, 
        mut wsender: mpsc::Sender<IncomingConnection>,
        mut receiver: mpsc::Receiver<RelayListenOut>, 
        mut sender: mpsc::Sender<RelayListenIn>,
        mut tick_sender: mpsc::Sender<()>,
        mut event_receiver: mpsc::Receiver<ListenerEvent>) {

        // Make sure that keepalives are sent on time:
        for i in 0 .. 8usize {
            await!(tick_sender.send(())).unwrap();
            let _ = await!(event_receiver.next()).unwrap();
        }

        let msg = await!(receiver.next()).unwrap();
        assert_eq!(msg, RelayListenOut::KeepAlive);

        // Check send/receive messages:
        let public_key = PublicKey::from(&[0xee; PUBLIC_KEY_LEN]);
        await!(wsender.send(IncomingConnection(public_key.clone())));
        let _ = await!(event_receiver.next()).unwrap();

        let msg = await!(receiver.next()).unwrap();
        assert_eq!(msg, RelayListenOut::IncomingConnection(IncomingConnection(public_key)));

        let public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        await!(sender.send(RelayListenIn::RejectConnection(RejectConnection(public_key.clone())))).unwrap();
        let _ = await!(event_receiver.next()).unwrap();

        let msg = await!(wreceiver.next()).unwrap();
        assert_eq!(msg, RejectConnection(public_key));

        // Check that lack of keepalives causes disconnection:
        for i in 0 .. 7usize {
            await!(tick_sender.send(())).unwrap();
            let _ = await!(event_receiver.next()).unwrap();
        }

        let public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        await!(sender.send(RelayListenIn::KeepAlive)).unwrap();
        let _ = await!(event_receiver.next()).unwrap();

        await!(tick_sender.send(())).unwrap();
        let _ = await!(event_receiver.next()).unwrap();

        let msg = await!(receiver.next()).unwrap();
        assert_eq!(msg, RelayListenOut::KeepAlive);

        for i in 0 .. 15usize {
            await!(tick_sender.send(())).unwrap();
            let _ = await!(event_receiver.next()).unwrap();
        }

        let msg = await!(receiver.next()).unwrap();
        assert_eq!(msg, RelayListenOut::KeepAlive);

        assert!(await!(receiver.next()).is_none());
        assert!(await!(wreceiver.next()).is_none());

        // Note: senders and receivers inside generators are seem to be dropped before the end of
        // the scope. Beware.
        // See also: https://users.rust-lang.org/t/sender-implicitly-dropped-inside-a-generator/20874
        // drop(wreceiver);
        drop(wsender);
        // drop(receiver);
        drop(sender);
    }

    #[test]
    fn test_listener_keepalive_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

        /*      a     c 
         * a_ca | <-- | c_ca
         *      |     | 
         * a_ac | --> | c_ac
        */

        let (a_ac, c_ac) = mpsc::channel::<RelayListenOut>(0);
        let (c_ca, a_ca) = mpsc::channel::<RelayListenIn>(0);

        let timer_stream = thread_pool.run(timer_client.request_timer_stream()).unwrap();

        let keepalive_ticks = 16;
        let (event_sender, event_receiver) = mpsc::channel(0);

        let (new_listener_sender, outgoing_messages) = mpsc::channel::<IncomingConnection>(0);
        let (incoming_messages, new_listener_receiver) = mpsc::channel::<RejectConnection>(0);
        let listener_fut = listener_loop(a_ca,
                      a_ac.sink_map_err(|_| ()),
                      outgoing_messages,
                      incoming_messages.sink_map_err(|_| ()),
                      timer_stream,
                      keepalive_ticks,
                      Some(event_sender));

        let spawn_fut = listener_fut.map_err(|e| {
            error!("listener_loop() error: {:?}", e);
            ()
        }).then(|_| future::ready(()));
        thread_pool.spawn(spawn_fut);
    
        let (w_a_ca, w_a_ac) = (new_listener_receiver, new_listener_sender);

        thread_pool.run(
            run_listener_keepalive_basic(
                w_a_ca, w_a_ac,
                c_ac, c_ca,
                tick_sender,
                event_receiver)
        );
    }
}
