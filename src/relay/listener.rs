#![allow(unused)]
use futures::prelude::{async, await}; 
use futures::{stream, Stream, Sink, Future};
use futures::sync::mpsc;
use timer::TimerClient;
use super::messages::{RelayListenOut, RelayListenIn,
                        RejectConnection, IncomingConnection};

enum ListenerLoopError {
    RequestTimerStreamError,
    TimerClosed,
    OutgoingMessages,
    ListenerReceiver,
    TimerStream,
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
                      listener_sender: K,
                      outgoing_messages: MO,
                      incoming_messages: KI,
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

    let mut ticks_to_close: usize = keepalive_ticks;
    let mut ticks_to_send_keepalive: usize = keepalive_ticks / 2;

    #[async]
    for listener_event in listener_events {
        match listener_event {
            ListenerEvent::ListenerReceiver(relay_listen_in) => {},
            ListenerEvent::OutgoingMessage(relay_listen_out) => {},
            ListenerEvent::ListenerReceiverClosed | 
            ListenerEvent::OutgoingMessagesClosed => break,
            ListenerEvent::TimerTick => {},
        }
    }

    Ok(())
}


pub fn listener_keepalive<M,K>(listener_receiver: M,
                      listener_sender: K,
                      timer_client: TimerClient,
                      keepalive_ticks: usize) 
                                       -> (impl Future<Item=(), Error=ListenerLoopError>, 
                                           impl Stream<Item=RejectConnection, Error=()>,
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
    
    (listener_fut,
     new_listener_receiver.map_err(|_| ()), 
     new_listener_sender.sink_map_err(|_| ()))
}
