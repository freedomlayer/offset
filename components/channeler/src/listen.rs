use std::marker::Unpin;
use futures::{select, future, FutureExt, Stream, StreamExt, Sink};
use futures::task::Spawn;

use proto::funder::messages::{FunderToChanneler, ChannelerToFunder};
use crypto::identity::PublicKey;
use timer::TimerClient;
use timer::utils::sleep_ticks;

use utils::int_convert::usize_to_u64;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::access_control::{AccessControlOp, AccessControl};

use crate::connector_utils::ConstAddressConnector;

#[derive(Debug)]
pub enum ListenError {
    RequestTimerStreamError,
    SleepTicksError,
    TimerClosed,
    AccessControlError,
    SpawnError,
    IncomingAddressClosed,
}

pub enum ListenSelect<A> {
    ListenError(ListenError),
    IncomingAddress(A),
    IncomingAddressClosed,
}

fn convert_client_listener_result(client_listener_result: Result<(), ClientListenerError>) -> Result<(), ListenError> {
    match client_listener_result {
        Ok(()) => unreachable!(),
        Err(ClientListenerError::RequestTimerStreamError) => 
            Err(ListenError::RequestTimerStreamError),
        Err(ClientListenerError::TimerClosed) => 
            Err(ListenError::TimerClosed),
        Err(ClientListenerError::AccessControlError) =>
            Err(ListenError::AccessControlError),
        Err(ClientListenerError::SpawnError) =>
            Err(ListenError::SpawnError),
        Err(ClientListenerError::SendInitConnectionError) |
        Err(ClientListenerError::ConnectionFailure) |
        Err(ClientListenerError::SendToServerError) |
        Err(ClientListenerError::ServerClosed) => Ok(()),
    }
}


/// Connect to relay and keep listening for incoming connections.
pub async fn listen_loop<A,C,IAC,CS,IA>(mut connector: C,
                 initial_address: A,
                 mut incoming_addresses: IA,
                 mut incoming_access_control: IAC,
                 connections_sender: CS,
                 conn_timeout_ticks: usize,
                 keepalive_ticks: usize,
                 backoff_ticks: usize,
                 timer_client: TimerClient,
                 spawner: impl Spawn + Clone + Send + 'static) -> Result<(), ListenError> 
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
            await!(sleep_ticks(backoff_ticks, timer_client.clone()))
                .map_err(|_| ListenError::SleepTicksError)?;
            Ok(())
        }).fuse();

        // TODO: Get rid of Box::pinned() later.
        let mut new_address_fut = Box::pinned(async {
            match await!(incoming_addresses.next()) {
                Some(address) => ListenSelect::IncomingAddress(address),
                None => ListenSelect::IncomingAddressClosed,
            }
        }).fuse();

        // TODO: Could we possibly lose an incoming address with this select?
        let listener_select = select! {
            listener_fut = listener_fut => ListenSelect::ListenError(listener_fut.err().unwrap()),
            new_address_fut = new_address_fut => new_address_fut,
        };
        // TODO: Make code nicer here somehow. Use scopes instead of drop?
        drop(new_address_fut);
        drop(listener_fut);

        match listener_select {
            ListenSelect::ListenError(listener_error) => Err(listener_error),
            ListenSelect::IncomingAddressClosed => Err(ListenError::IncomingAddressClosed),
            ListenSelect::IncomingAddress(new_address) => {
                address = new_address;
                Ok(())
            }
        }?;
    }
}

