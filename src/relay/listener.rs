#![allow(unused)]
use futures::prelude::{async, await}; 
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;
use timer::TimerClient;
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

enum ListenerEvent {
    ListenerReceiver(RelayListenIn),
    OutgoingMessage(IncomingConnection),
    ListenerReceiverClosed,
    OutgoingMessagesClosed,
    TimerTick,
}

#[async]
fn listener_loop<M,K,MO,KI>(listener_receiver: M,
                      mut listener_sender: K,
                      outgoing_messages: MO,
                      mut incoming_messages: KI,
                      timer_client: TimerClient, 
                      keepalive_ticks: usize) 
    -> Result<(), ListenerLoopError> 
where
    M: Stream<Item=RelayListenIn,Error=()> + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=()> + 'static,
    MO: Stream<Item=IncomingConnection,Error=()> + 'static,
    KI: Sink<SinkItem=RejectConnection,SinkError=()> + 'static,
{
    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| ListenerLoopError::RequestTimerStreamError)?;
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
pub fn listener_keepalive<M,K>(listener_receiver: M,
                      listener_sender: K,
                      timer_client: TimerClient,
                      keepalive_ticks: usize,
                      handle: &Handle) 
                                       -> (impl Stream<Item=RejectConnection, Error=()>,
                                           impl Sink<SinkItem=IncomingConnection, SinkError=()>)
where
    M: Stream<Item=RelayListenIn,Error=()> + 'static,
    K: Sink<SinkItem=RelayListenOut,SinkError=()> + 'static,
{
    let (new_listener_sender, outgoing_messages) = mpsc::channel::<IncomingConnection>(0);
    let (incoming_messages, new_listener_receiver) = mpsc::channel::<RejectConnection>(0);
    let listener_fut = listener_loop(listener_receiver,
                  listener_sender,
                  outgoing_messages.map_err(|_| ()),
                  incoming_messages.sink_map_err(|_| ()),
                  timer_client,
                  keepalive_ticks);

    handle.spawn(listener_fut.map_err(|e| {
        error!("listener_loop() error: {:?}", e);
        ()
    }));
    
     (new_listener_receiver.map_err(|_| ()), 
     new_listener_sender.sink_map_err(|_| ()))
}
