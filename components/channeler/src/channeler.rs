use std::marker::Unpin;
use futures::{Stream, Sink};
use futures::task::Spawn;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use relay::client::connector::{Connector, ConnPair};
use relay::client::access_control::AccessControlOp;


/// Connect to relay and keep listening for incoming connections.
fn listener_loop<C,IAC,CS>(mut connector: C,
                 incoming_access_control: IAC,
                 connections_sender: CS,
                 conn_timeout_ticks: usize,
                 keepalive_ticks: usize,
                 timer_client: TimerClient,
                 spawner: impl Spawn + Clone + Send + 'static) 
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>>, 
    IAC: Stream<Item=AccessControlOp> + Unpin,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
{
    unimplemented!();

}


fn inner_channeler_loop<FF,TF,C,A>(address: A,
                        from_funder: FF, 
                        to_funder: TF,
                        timer_client: TimerClient,
                        connector: C,
                        mut spawner: impl Spawn + Clone + Send + 'static)
where
    A: Clone,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    FF: Stream<Item=FunderToChanneler<A>>,
    TF: Sink<SinkItem=ChannelerToFunder>,
{
    unimplemented!();
    // TODO:
    // Loop:
    // - Attempt to connect to relay using given address
}


