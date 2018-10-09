#![allow(unused)]
use futures::prelude::{async, await}; 
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;
use timer::TimerTick;
use super::messages::{RelayListenOut, RelayListenIn,
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

#[derive(Debug)]
enum ListenerEvent {
    ListenerReceiver(RelayListenIn),
    OutgoingMessage(IncomingConnection),
    ListenerReceiverClosed,
    OutgoingMessagesClosed,
    TimerTick,
}

#[async]
fn listener_loop<M,K,MO,KI,TS>(listener_receiver: M,
                      mut listener_sender: K,
                      outgoing_messages: MO,
                      mut incoming_messages: KI,
                      timer_stream: TS, 
                      keepalive_ticks: usize) 
    -> Result<(), ListenerLoopError> 
where
    M: Stream<Item=RelayListenIn,Error=()> + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=()> + 'static,
    MO: Stream<Item=IncomingConnection,Error=()> + 'static,
    KI: Sink<SinkItem=RejectConnection,SinkError=()> + 'static,
    TS: Stream<Item=TimerTick,Error=()> + 'static,
{
    let timer_stream = timer_stream
        .map_err(|_| ListenerLoopError::TimerStream)
        .map(|_| ListenerEvent::TimerTick)
        .chain(stream::once(Err(ListenerLoopError::TimerClosed)));

    let outgoing_messages = outgoing_messages
        .map_err(|_| ListenerLoopError::OutgoingMessages)
        .map(|incoming_connection| ListenerEvent::OutgoingMessage(incoming_connection))
        .chain(stream::once(Ok(ListenerEvent::OutgoingMessagesClosed)));

    let listener_receiver = listener_receiver
        .map_err(|_| ListenerLoopError::ListenerReceiver)
        .map(|relay_listen_in| ListenerEvent::ListenerReceiver(relay_listen_in))
        .chain(stream::once(Ok(ListenerEvent::ListenerReceiverClosed)));

    let listener_events = timer_stream
        .select(outgoing_messages)
        .select(listener_receiver);

    // When this amount reaches zero, we decide that the remote side is not alive and we close the
    // connection:
    let mut ticks_to_close: usize = keepalive_ticks;
    // When this amount reaches zero, we need to send a keepalive to prove to the remote side that
    // we are alive:
    let mut ticks_to_send_keepalive: usize = keepalive_ticks / 2;


    #[async]
    for listener_event in listener_events {
        match listener_event {
            ListenerEvent::ListenerReceiver(relay_listen_in) => {
                match relay_listen_in {
                    RelayListenIn::KeepAlive => {},
                    RelayListenIn::RejectConnection(reject_connection) => {
                        incoming_messages = await!(incoming_messages.send(reject_connection))
                            .map_err(|_| ListenerLoopError::IncomingMessagesFailure)?;
                    },
                }
                ticks_to_close = keepalive_ticks;
            },
            ListenerEvent::OutgoingMessage(incoming_connection) => {
                let relay_listen_out = RelayListenOut::IncomingConnection(incoming_connection);
                listener_sender = await!(listener_sender.send(relay_listen_out))
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
                    listener_sender = await!(listener_sender.send(RelayListenOut::KeepAlive))
                        .map_err(|_| ListenerLoopError::ListenerSenderFailure)?;
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            },
        }
    }

    Ok(())
}


/// Deal with keepalive logic of a listener connection.
/// Returns back a pair of sender and receiver, stripped from keepalive logic.
pub fn listener_keepalive<M,K,ME,KE,TS>(listener_receiver: M,
                      listener_sender: K,
                      timer_stream: TS,
                      keepalive_ticks: usize,
                      handle: &Handle) 
                                       -> (mpsc::Receiver<RejectConnection>,
                                           mpsc::Sender<IncomingConnection>)
where
    M: Stream<Item=RelayListenIn,Error=ME> + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=KE> + 'static,
    TS: Stream<Item=TimerTick,Error=()> + 'static,
{
    let (new_listener_sender, outgoing_messages) = mpsc::channel::<IncomingConnection>(0);
    let (incoming_messages, new_listener_receiver) = mpsc::channel::<RejectConnection>(0);
    let listener_fut = listener_loop(listener_receiver.map_err(|_| ()),
                  listener_sender.sink_map_err(|_| ()),
                  outgoing_messages.map_err(|_| ()),
                  incoming_messages.sink_map_err(|_| ()),
                  timer_stream,
                  keepalive_ticks);

    handle.spawn(listener_fut.map_err(|e| {
        error!("listener_loop() error: {:?}", e);
        ()
    }));
    
    (new_listener_receiver, new_listener_sender)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::prelude::{async, await};
    use futures::sync::{mpsc, oneshot};
    use futures::Future;
    use tokio_core::reactor::Core;
    use timer::create_timer_incoming;
    use utils::async_test_utils::{receive, ReceiveError};
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    #[async]
    fn run_listener_keepalive_basic(
        mut wreceiver: mpsc::Receiver<RejectConnection>, 
        mut wsender: mpsc::Sender<IncomingConnection>,
        mut receiver: mpsc::Receiver<RelayListenOut>, 
        mut sender: mpsc::Sender<RelayListenIn>,
        mut tick_sender: mpsc::Sender<()>) -> Result<(),()> {

        // Make sure that keepalives are sent on time:
        for i in 0 .. 8usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }

        let (msg, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(msg, RelayListenOut::KeepAlive);

        // Check send/receive messages:
        let public_key = PublicKey::from(&[0xee; PUBLIC_KEY_LEN]);
        wsender = await!(wsender.send(IncomingConnection(public_key.clone()))).unwrap();

        let (msg, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(msg, RelayListenOut::IncomingConnection(IncomingConnection(public_key)));

        let public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        sender = await!(sender.send(RelayListenIn::RejectConnection(RejectConnection(public_key.clone())))).unwrap();

        let (msg, new_wreceiver) = await!(receive(wreceiver)).unwrap();
        wreceiver = new_wreceiver;
        assert_eq!(msg, RejectConnection(public_key));

        // Check that lack of keepalives causes disconnection:
        for i in 0 .. 7usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }

        let public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        sender = await!(sender.send(RelayListenIn::KeepAlive)).unwrap();

        tick_sender = await!(tick_sender.send(())).unwrap();

        let (msg, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(msg, RelayListenOut::KeepAlive);

        for i in 0 .. 15usize {
            tick_sender = await!(tick_sender.send(())).unwrap();
        }

        let (msg, new_receiver) = await!(receive(receiver)).unwrap();
        receiver = new_receiver;
        assert_eq!(msg, RelayListenOut::KeepAlive);

        if let Err(ReceiveError::Closed) = await!(receive(receiver)) {
        } else { 
            unreachable!();
        }

        if let Err(ReceiveError::Closed) = await!(receive(wreceiver)) {
        } else { 
            unreachable!();
        }

        // Note: senders and receivers inside generators are seem to be dropped before the end of
        // the scope. Beware.
        // See also: https://users.rust-lang.org/t/sender-implicitly-dropped-inside-a-generator/20874
        // drop(wreceiver);
        drop(wsender);
        // drop(receiver);
        drop(sender);

        Ok(())
    }

    #[test]
    fn test_listener_keepalive_basic() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();

        // Create a mock time service:
        let (tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, &handle).unwrap();

        /*      a     c 
         * a_ca | <-- | c_ca
         *      |     | 
         * a_ac | --> | c_ac
        */

        let (a_ac, c_ac) = mpsc::channel::<RelayListenOut>(0);
        let (c_ca, a_ca) = mpsc::channel::<RelayListenIn>(0);

        let timer_stream = core.run(timer_client.request_timer_stream()).unwrap();

        let keepalive_ticks = 16;
        let (w_a_ca, w_a_ac) = listener_keepalive(a_ca, 
                                  a_ac,
                                  timer_stream,
                                  keepalive_ticks,
                                  &handle);

        core.run(
            run_listener_keepalive_basic(
                w_a_ca, w_a_ac,
                c_ac, c_ca,
                tick_sender)
        ).unwrap();
    }
}
