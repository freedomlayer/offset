use std::marker::Unpin;

use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use crypto::identity::PublicKey;
use proto::relay::messages::{InitConnection, RelayListenOut, RelayListenIn, IncomingConnection};
use proto::relay::serialize::{serialize_init_connection,
    serialize_relay_listen_in, deserialize_relay_listen_out};

use timer::TimerClient;
use super::connector::{Connector, ConnPair};
use super::access_control::{AccessControl, AccessControlOp};


#[derive(Debug)]
pub enum ClientListenerError {
    RequestTimerStreamError,
    SendInitConnectionError,
    ConnectionFailure,
    ConnectionTimedOut,
    TimerClosed,
    AccessControlError,
    SendToServerError,
    ServerClosed,
}


#[derive(Debug, Clone)]
enum ClientListenerEvent {
    TimerTick,
    TimerClosed,
    AccessControlOp(AccessControlOp),
    AccessControlClosed,
    ServerMessage(RelayListenOut),
    ServerClosed,
}

async fn inner_client_listener<C,IAC,CS,CSE>(mut connector: C,
                                incoming_access_control: IAC,
                                connections_sender: CS,
                                keepalive_ticks: usize,
                                mut timer_client: TimerClient,
                                spawner: impl Spawn,
                                mut opt_event_sender: Option<mpsc::Sender<ClientListenerEvent>>) 
    -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>>,
    IAC: Stream<Item=AccessControlOp> + Unpin,
    CS: Sink<SinkItem=ConnPair<Vec<u8>, Vec<u8>>, SinkError=CSE> + Unpin,
{
    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_|  ClientListenerError::RequestTimerStreamError)?;

    let conn_pair = match await!(connector.connect(())) {
        Some(conn_pair) => conn_pair,
        None => return Err(ClientListenerError::ConnectionFailure),
    };

    let ConnPair {mut sender, receiver} = conn_pair;
    let ser_init_connection = serialize_init_connection(&InitConnection::Listen);

    await!(sender.send(ser_init_connection))
        .map_err(|_| ClientListenerError::SendInitConnectionError)?;

    // Add serialization for sender:
    let mut sender = sender
        .sink_map_err(|_| ())
        .with(|vec| -> future::Ready<Result<_,()>> {
            future::ready(Ok(serialize_relay_listen_in(&vec)))
        });

    // Add deserialization for receiver:
    let mut receiver = receiver.map(|relay_listen_out| {
        deserialize_relay_listen_out(&relay_listen_out).ok()
    }).take_while(|opt_relay_listen_out| {
        future::ready(opt_relay_listen_out.is_some())
    }).map(|opt| opt.unwrap());

    let mut access_control = AccessControl::new();
    // Amount of ticks remaining until we decide to close this connection (Because remote is idle):
    let mut ticks_to_close = keepalive_ticks;
    // Amount of ticks remaining until we need to send a new keepalive (To make sure remote side
    // knows we are alive).
    let mut ticks_to_send_keepalive = keepalive_ticks / 2;


    let timer_stream = timer_stream
        .map(|_| ClientListenerEvent::TimerTick)
        .chain(stream::once(future::ready(ClientListenerEvent::TimerClosed)));

    let incoming_access_control = incoming_access_control
        .map(|access_control_op| ClientListenerEvent::AccessControlOp(access_control_op))
        .chain(stream::once(future::ready(ClientListenerEvent::AccessControlClosed)));

    let server_receiver = receiver
        .map(ClientListenerEvent::ServerMessage)
        .chain(stream::once(future::ready(ClientListenerEvent::ServerClosed)));

    let mut events = timer_stream
        .select(incoming_access_control)
        .select(server_receiver);

    while let Some(event) = await!(events.next()) {
        if let Some(ref mut event_sender) = opt_event_sender {
            await!(event_sender.send(event.clone()));
        }
        match event {
            ClientListenerEvent::TimerTick => {
                ticks_to_close = ticks_to_close.saturating_sub(1);
                ticks_to_send_keepalive = ticks_to_send_keepalive.saturating_sub(1);
                if ticks_to_close == 0 {
                    break;
                }
                if ticks_to_send_keepalive == 0 {
                    await!(sender.send(RelayListenIn::KeepAlive))
                        .map_err(|_| ClientListenerError::SendToServerError);
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            },
            ClientListenerEvent::TimerClosed => return Err(ClientListenerError::TimerClosed),
            ClientListenerEvent::AccessControlOp(access_control_op) => {
                access_control.apply_op(access_control_op)
                    .map_err(|_| ClientListenerError::AccessControlError)?;
            },
            ClientListenerEvent::AccessControlClosed => {}, // Don't care?
            ClientListenerEvent::ServerMessage(relay_listen_out) => {
                ticks_to_close = keepalive_ticks;
                match relay_listen_out {
                    RelayListenOut::KeepAlive => {},
                    RelayListenOut::IncomingConnection(IncomingConnection(public_key)) => {
                        // TODO:
                        // handle_incoming_connection(...)
                        // Possibly create a separate function for this part.
                    },
                }
            },
            ClientListenerEvent::ServerClosed => return Err(ClientListenerError::ServerClosed),
        }
    }
    Ok(())

    // TODO: 
    // - Create a new connection to the relay.
    // - Send an Init::Listen message, to turn the connection into a listen connection.

}

