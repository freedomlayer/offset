use std::marker::Unpin;
use futures::{future, Stream, StreamExt, Sink};
use futures::task::Spawn;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::access_control::{AccessControlOp, AccessControl};

pub enum ListenerEvent {
    TimerTick,
    RelayListenerClosed,
    IncomingAccessControl(AccessControlOp),
}

pub enum ListenerError {
    RequestTimerStreamError,
    TimerClosed,
    AccessControlError,
    SpawnError,
}

fn convert_client_listener_result(client_listener_result: Result<(), ClientListenerError>) -> Result<(), ListenerError> {
    match client_listener_result {
        Ok(()) => unreachable!(),
        Err(ClientListenerError::RequestTimerStreamError) => 
            Err(ListenerError::RequestTimerStreamError),
        Err(ClientListenerError::TimerClosed) => 
            Err(ListenerError::TimerClosed),
        Err(ClientListenerError::AccessControlError) =>
            Err(ListenerError::AccessControlError),
        Err(ClientListenerError::SpawnError) =>
            Err(ListenerError::SpawnError),
        Err(ClientListenerError::SendInitConnectionError) |
        Err(ClientListenerError::ConnectionFailure) |
        Err(ClientListenerError::SendToServerError) |
        Err(ClientListenerError::ServerClosed) => Ok(()),
    }
}

async fn sleep_ticks(ticks: usize, mut timer_client: TimerClient) -> Result<(), ListenerError> {
    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| ListenerError::RequestTimerStreamError)?;
    let ticks_u64 = usize_to_u64(ticks).unwrap();
    let fut = timer_stream
        .take(ticks_u64)
        .for_each(|_| future::ready(()));
    Ok(await!(fut))
}

/// Connect to relay and keep listening for incoming connections.
async fn listener_loop<C,IAC,CS>(mut connector: C,
                 mut incoming_access_control: IAC,
                 connections_sender: CS,
                 conn_timeout_ticks: usize,
                 keepalive_ticks: usize,
                 backoff_ticks: usize,
                 timer_client: TimerClient,
                 spawner: impl Spawn + Clone + Send + 'static) -> Result<(), ListenerError> 
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Sync + Send + Clone + 'static, 
    IAC: Stream<Item=AccessControlOp> + Unpin + 'static,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
{
    let mut access_control = AccessControl::new();
    loop {
        let res = await!(client_listener(connector.clone(),
                                    &mut access_control,
                                    &mut incoming_access_control,
                                    connections_sender.clone(),
                                    conn_timeout_ticks,
                                    keepalive_ticks,
                                    timer_client.clone(),
                                    spawner.clone()));

        // Exit the loop if the error is fatal:
        // TODO: Should we unwrap here? We need to make sure that everything stops if a fatal error
        // occurs here.
        convert_client_listener_result(res)?;

        // Wait for a while before attempting to connect again:
        // TODO: Possibly wait here in a smart way? Exponential backoff?
        await!(sleep_ticks(backoff_ticks, timer_client.clone()))?;
    }
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


