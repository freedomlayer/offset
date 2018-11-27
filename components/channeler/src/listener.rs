use std::marker::Unpin;
use futures::{select, future, FutureExt, Stream, StreamExt, Sink};
use futures::task::Spawn;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::access_control::{AccessControlOp, AccessControl};

use crate::connector_utils::ConstAddressConnector;

#[derive(Debug)]
pub enum ListenerError {
    RequestTimerStreamError,
    TimerClosed,
    AccessControlError,
    SpawnError,
    IncomingAddressClosed,
}

pub enum ListenerSelect<A> {
    ListenerError(ListenerError),
    IncomingAddress(A),
    IncomingAddressClosed,
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
pub async fn listener_loop<A,C,IAC,CS,IA>(mut connector: C,
                 initial_address: A,
                 mut incoming_addresses: IA,
                 mut incoming_access_control: IAC,
                 connections_sender: CS,
                 conn_timeout_ticks: usize,
                 keepalive_ticks: usize,
                 backoff_ticks: usize,
                 timer_client: TimerClient,
                 spawner: impl Spawn + Clone + Send + 'static) -> Result<(), ListenerError> 
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Sync + Send + Clone + 'static, 
    IA: Stream<Item=A> + Unpin + 'static,
    IAC: Stream<Item=AccessControlOp> + Unpin + 'static,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
{
    let mut access_control = AccessControl::new();
    let mut address = initial_address;
    loop {
        // TODO: get rid of Box::pinned() later.
        let mut listener_fut = Box::pinned(async {
            let const_address_connector = ConstAddressConnector::new(connector.clone(), address.clone());
            let res = await!(client_listener(const_address_connector,
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
            Ok(())
        }).fuse();

        // TODO: Get rid of Box::pinned() later.
        let mut new_address_fut = Box::pinned(async {
            match await!(incoming_addresses.next()) {
                Some(address) => ListenerSelect::IncomingAddress(address),
                None => ListenerSelect::IncomingAddressClosed,
            }
        }).fuse();

        // TODO: Could we possibly lose an incoming address with this select?
        let listener_select = select! {
            listener_fut = listener_fut => ListenerSelect::ListenerError(listener_fut.err().unwrap()),
            new_address_fut = new_address_fut => new_address_fut,
        };
        // TODO: Make code nicer here somehow. Use scopes instead of drop?
        drop(new_address_fut);
        drop(listener_fut);

        match listener_select {
            ListenerSelect::ListenerError(listener_error) => Err(listener_error),
            ListenerSelect::IncomingAddressClosed => Err(ListenerError::IncomingAddressClosed),
            ListenerSelect::IncomingAddress(new_address) => {
                address = new_address;
                Ok(())
            }
        }?;
    }
}

