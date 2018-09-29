#![allow(unused)]
use futures::prelude::{async, await}; 
use futures::{Stream, Sink, Future};
use futures::sync::mpsc;
use timer::TimerClient;
use super::messages::{RelayListenOut, RelayListenIn,
                        RejectConnection, IncomingConnection};

enum ListenerEvent {
    ListenerReceiver(RelayListenIn),
    ListenerReceiverClosed,
    TimerTick,
    TimerTickClosed,
    OutgoingMessage(RelayListenOut),
    OutgoingMessagesClosed,
}

#[async]
fn listener_loop<M,K,MO,KI>(listener_receiver: M,
                      listener_sender: K,
                      outgoing_messages: MO,
                      incoming_messages: KI,
                      timer_client: TimerClient, 
                      keepalive_ticks: usize) 
    -> Result<(), ()> 
where
    M: Stream<Item=RelayListenIn,Error=()>,
    K: Sink<SinkItem=RelayListenOut,SinkError=()>,
    MO: Stream<Item=IncomingConnection,Error=()>,
    KI: Sink<SinkItem=RejectConnection,SinkError=()>,
{
    let mut ticks_to_close: usize = keepalive_ticks;
    let mut ticks_to_send_keepalive: usize = keepalive_ticks / 2;
    unimplemented!();
}


pub fn listener_keepalive<M,K>(listener_receiver: M,
                      listener_sender: K,
                      timer_client: TimerClient,
                      keepalive_ticks: usize) 
                                       -> (impl Future<Item=(), Error=()>, 
                                           impl Stream<Item=RejectConnection, Error=()>,
                                           impl Sink<SinkItem=IncomingConnection, SinkError=()>)
where
    M: Stream<Item=RelayListenIn,Error=()>,
    K: Sink<SinkItem=RelayListenOut,SinkError=()>,
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
