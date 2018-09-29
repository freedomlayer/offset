#![allow(unused)]
use futures::prelude::{async, await}; 
use futures::{Stream, Sink};
use super::messages::{RelayListenOut, RelayListenIn};

enum ListenerEvent {
}

#[async]
fn listener_loop<M,K>(listener_receiver: M,
                      listener_sender: K,
                      keepalive_ticks: usize) 
    -> Result<(), ()> 
where
    M: Stream<Item=RelayListenIn,Error=()>,
    K: Sink<SinkItem=RelayListenOut,SinkError=()>,
{
    let mut ticks_to_close: usize = keepalive_ticks;
    let mut ticks_to_send_keepalive: usize = keepalive_ticks / 2;
    unimplemented!();
}
