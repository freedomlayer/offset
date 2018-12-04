use std::marker::Unpin;
use futures::{select, FutureExt, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{mpsc, oneshot};

use proto::consts::TICKS_TO_REKEY;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use timer::TimerClient;
use timer::utils::sleep_ticks;

use identity::IdentityClient;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_listener::{client_listener, ClientListenerError};
use relay::client::access_control::{AccessControlOp, AccessControl};

use secure_channel::create_secure_channel;

use crate::connector_utils::ConstAddressConnector;

#[derive(Debug)]
pub enum ListenError {
    RequestTimerStreamError,
    SleepTicksError,
    TimerClosed,
    AccessControlError,
    SpawnError,
    IncomingAddressClosed,
    Canceled,
}

enum ListenSelect {
    ListenError(ListenError),
    Canceled,
}

/// Distinguish between fatal and non-fatal errors
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

/// Encrypt incoming plain connections
///
/// We require this logic because it is inefficient to perform handshake for the connections serially.
/// For example: It is possible that connection A arrives before connection B, but performing
/// handshake for A takes longer than it takes for B.
pub async fn conn_encryptor<CS,R,S>(mut plain_connections_receiver: mpsc::Receiver<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
                            encrypted_connections_sender: CS,
                            timer_client: TimerClient,
                            identity_client: IdentityClient,
                            rng: R,
                            mut spawner: S)
where
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + 'static,
{
    while let Some((public_key, conn_pair)) = await!(plain_connections_receiver.next()) {
        let secure_channel_fut = create_secure_channel(conn_pair.sender, conn_pair.receiver,
                              identity_client.clone(),
                              Some(public_key.clone()),
                              rng.clone(),
                              timer_client.clone(),
                              TICKS_TO_REKEY,
                              spawner.clone());
        let mut c_encrypted_connections_sender = encrypted_connections_sender.clone();
        spawner.spawn(async move {
            match await!(secure_channel_fut) {
                Ok((sender, receiver)) => {
                    let conn_pair = ConnPair {sender, receiver};
                    if let Err(_e) = await!(c_encrypted_connections_sender.send((public_key, conn_pair))) {
                        error!("conn_encryptor(): Can not send through encrypted_connections_sender");
                    }
                },
                Err(e) => 
                    error!("conn_encryptor(): error in create_secure_channel(): {:?}", e),
            }
        }).unwrap();
    }
}


/// Connect to relay and keep listening for incoming connections.
/// Aborts in one of the following cases:
/// - Canceled through close_receiver.
/// - A fatal error occured.
pub async fn listen_loop<A,C,IAC,CS,R,S>(connector: C,
                 address: A,
                 mut incoming_access_control: IAC,
                 connections_sender: CS,
                 close_receiver: oneshot::Receiver<()>,
                 mut access_control: AccessControl,
                 conn_timeout_ticks: usize,
                 keepalive_ticks: usize,
                 backoff_ticks: usize,
                 timer_client: TimerClient,
                 identity_client: IdentityClient,
                 rng: R,
                 mut spawner: S) -> Result<(), ListenError> 
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Sync + Send + Clone + 'static, 
    IAC: Stream<Item=AccessControlOp> + Unpin + 'static,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let (plain_connections_sender, plain_connections_receiver) = mpsc::channel(0);
    spawner.spawn(conn_encryptor(plain_connections_receiver, 
                                 connections_sender, // Sends encrypted connections
                                 timer_client.clone(),
                                 identity_client,
                                 rng,
                                 spawner.clone()))
        .map_err(|_| ListenError::SpawnError)?;

    // TODO: get rid of Box::pinned() later ?
    let mut listener_fut = Box::pinned(async {
        loop {
            let const_address_connector = ConstAddressConnector::new(connector.clone(), address.clone());
            let res = await!(client_listener(const_address_connector,
                                        &mut access_control,
                                        &mut incoming_access_control,
                                        plain_connections_sender.clone(),
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
        }
        // This is a hack to help the compiler know that the return type
        // here is Result<(),_>
        #[allow(unreachable_code)]
        Ok(())
    }).fuse();

    let mut close_fut = close_receiver.fuse();

    let listener_select = select! {
        listener_fut = listener_fut => ListenSelect::ListenError(listener_fut.err().unwrap()),
        _close_fut = close_fut => ListenSelect::Canceled,
    };

    match listener_select {
        ListenSelect::ListenError(listener_error) => Err(listener_error),
        ListenSelect::Canceled => Err(ListenError::Canceled),
    }
}

