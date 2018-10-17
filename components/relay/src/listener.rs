#![allow(unused)]
use std::marker::Unpin;
use futures::{future, Future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use timer::TimerTick;
use proto::relay::messages::{RelayListenOut, RelayListenIn,
                        RejectConnection, IncomingConnection};

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
    ListenerReceiver(RelayListenIn),
    OutgoingMessage(IncomingConnection),
    ListenerReceiverClosed,
    OutgoingMessagesClosed,
    TimerTick,
    TimerClosed,
}

async fn listener_loop<M,K,MO,KI,TS>(listener_receiver: M,
                      mut listener_sender: K,
                      outgoing_messages: MO,
                      mut incoming_messages: KI,
                      timer_stream: TS, 
                      keepalive_ticks: usize,
                      mut opt_event_sender: Option<mpsc::Sender<ListenerEvent>>)
    -> Result<(), ListenerLoopError> 
where
    M: Stream<Item=RelayListenIn> + Unpin + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=()> + Unpin + 'static,
    MO: Stream<Item=IncomingConnection> + Unpin + 'static,
    KI: Sink<SinkItem=RejectConnection,SinkError=()> + Unpin + 'static,
    TS: Stream<Item=TimerTick> + Unpin + 'static,
{
    let timer_stream = timer_stream
        .map(|_| ListenerEvent::TimerTick)
        .chain(stream::once(future::ready(ListenerEvent::TimerClosed)));

    let mut outgoing_messages = outgoing_messages
        .map(|incoming_connection| ListenerEvent::OutgoingMessage(incoming_connection))
        .chain(stream::once(future::ready(ListenerEvent::OutgoingMessagesClosed)));

    let mut listener_receiver = listener_receiver
        .map(|relay_listen_in| ListenerEvent::ListenerReceiver(relay_listen_in))
        .chain(stream::once(future::ready(ListenerEvent::ListenerReceiverClosed)));

    let mut listener_events = timer_stream
        .select(outgoing_messages)
        .select(listener_receiver);

    // When this amount reaches zero, we decide that the remote side is not alive and we close the
    // connection:
    let mut ticks_to_close: usize = keepalive_ticks;
    // When this amount reaches zero, we need to send a keepalive to prove to the remote side that
    // we are alive:
    let mut ticks_to_send_keepalive: usize = keepalive_ticks / 2;


    while let Some(listener_event) = await!(listener_events.next()) {
        if let Some(ref mut event_sender) = opt_event_sender {
            await!(event_sender.send(listener_event.clone())).unwrap();
        }
        match listener_event {
            ListenerEvent::ListenerReceiver(relay_listen_in) => {
                match relay_listen_in {
                    RelayListenIn::KeepAlive => {},
                    RelayListenIn::RejectConnection(reject_connection) => {
                        await!(incoming_messages.send(reject_connection))
                            .map_err(|_| ListenerLoopError::IncomingMessagesFailure)?;
                    },
                }
                ticks_to_close = keepalive_ticks;
            },
            ListenerEvent::OutgoingMessage(incoming_connection) => {
                let relay_listen_out = RelayListenOut::IncomingConnection(incoming_connection);
                await!(listener_sender.send(relay_listen_out))
                    .map_err(|_| ListenerLoopError::ListenerSenderFailure)?;
                ticks_to_send_keepalive = keepalive_ticks / 2;
            },
            ListenerEvent::ListenerReceiverClosed | 
            ListenerEvent::OutgoingMessagesClosed => break,
            ListenerEvent::TimerTick => {
                ticks_to_close = ticks_to_close.saturating_sub(1);
                if ticks_to_close == 0 {
                    break;
                }
                ticks_to_send_keepalive 
                    = ticks_to_send_keepalive.saturating_sub(1);
                if ticks_to_send_keepalive == 0 {
                    await!(listener_sender.send(RelayListenOut::KeepAlive))
                        .map_err(|_| ListenerLoopError::ListenerSenderFailure)?;
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            },
            ListenerEvent::TimerClosed => return Err(ListenerLoopError::TimerClosed),
        }
    }

    Ok(())
}


/// Deal with keepalive logic of a listener connection.
/// Returns back a pair of sender and receiver, stripped from keepalive logic.
pub fn listener_keepalive<M,K,KE,TS>(listener_receiver: M,
                      listener_sender: K,
                      timer_stream: TS,
                      keepalive_ticks: usize,
                      mut spawner: impl Spawn)
                                       -> (mpsc::Receiver<RejectConnection>,
                                           mpsc::Sender<IncomingConnection>)
where
    M: Stream<Item=RelayListenIn> + Unpin + Send + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=KE> + Unpin + Send + 'static,
    TS: Stream<Item=TimerTick> + Unpin + Send + 'static,
{
    let (new_listener_sender, outgoing_messages) = mpsc::channel::<IncomingConnection>(0);
    let (incoming_messages, new_listener_receiver) = mpsc::channel::<RejectConnection>(0);
    let listener_fut = listener_loop(listener_receiver,
                  listener_sender.sink_map_err(|_| ()),
                  outgoing_messages,
                  incoming_messages.sink_map_err(|_| ()),
                  timer_stream,
                  keepalive_ticks,
                  None);

    let spawn_fut = listener_fut.map_err(|e| {
        error!("listener_loop() error: {:?}", e);
        ()
    }).then(|_| future::ready(()));
    spawner.spawn(spawn_fut);
    
    (new_listener_receiver, new_listener_sender)
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
        let timer_client = create_timer_incoming(tick_receiver, thread_pool.clone()).unwrap();

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
