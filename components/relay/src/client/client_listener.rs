use std::marker::Unpin;

use futures::{future, Future, FutureExt, TryFutureExt, 
    stream, Stream, StreamExt, Sink, SinkExt,
    select};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use crypto::identity::PublicKey;
use proto::relay::messages::{InitConnection, RelayListenOut, RelayListenIn, 
    IncomingConnection, RejectConnection};
use proto::relay::serialize::{serialize_init_connection,
    serialize_relay_listen_in, deserialize_relay_listen_out,
    serialize_tunnel_message, deserialize_tunnel_message};
use utils::int_convert::usize_to_u64;

use timer::{TimerClient, TimerTick};
use super::connector::{Connector, ConnPair};
use super::access_control::{AccessControl, AccessControlOp};
use super::client_tunnel::client_tunnel;


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
    SpawnError,
}


#[derive(Debug, Clone)]
enum ClientListenerEvent {
    TimerTick,
    TimerClosed,
    AccessControlOp(AccessControlOp),
    ServerMessage(RelayListenOut),
    ServerClosed,
    PendingReject(PublicKey),
}

#[derive(Debug)]
enum AcceptConnectionError {
    ConnectionFailed,
    PendingRejectSenderError,
    SendInitConnectionError,
    SendConnPairError,
    RequestTimerStreamError,
    SpawnError,
}

/*
/// Convert a Sink to an mpsc::Sender<T>
/// This is done to overcome some compiler type limitations.
fn to_mpsc_sender<T,SI,SE>(mut sink: SI, mut spawner: impl Spawn) -> mpsc::Sender<T> 
where
    SI: Sink<SinkItem=T, SinkError=SE> + Unpin + Send + 'static,
    T: Send + 'static,
{
    let (sender, mut receiver) = mpsc::channel::<T>(0);
    let fut = async move {
        await!(sink.send_all(&mut receiver))
    }.map(|_| ());
    spawner.spawn(fut).unwrap();
    sender
}

/// Convert a Stream to an mpsc::Receiver<T>
/// This is done to overcome some compiler type limitations.
fn to_mpsc_receiver<T,ST,SE>(mut stream: ST, mut spawner: impl Spawn) -> mpsc::Receiver<T> 
where
    ST: Stream<Item=T> + Unpin + Send + 'static,
    T: Send + 'static,
{
    let (mut sender, receiver) = mpsc::channel::<T>(0);
    let fut = async move {
        await!(sender.send_all(&mut stream))
    }.map(|_| ());
    spawner.spawn(fut).unwrap();
    receiver
}
*/

async fn connect_with_timeout<C,TS>(connector: C,
                       conn_timeout_ticks: usize,
                       timer_stream: TS) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send,
    TS: Stream<Item=TimerTick> + Unpin,
{

    let conn_timeout_ticks = usize_to_u64(conn_timeout_ticks).unwrap();
    let mut fut_timeout = timer_stream
        .take(conn_timeout_ticks)
        .for_each(|_| future::ready(()));
    let mut fut_connect = connector.connect(());

    select! {
        fut_timeout => None,
        fut_connect => fut_connect,
    }
}

async fn accept_connection<C,CS, CSE>(public_key: PublicKey, 
                           connector: C,
                           mut pending_reject_sender: mpsc::Sender<PublicKey>,
                           mut connections_sender: CS,
                           conn_timeout_ticks: usize,
                           keepalive_ticks: usize,
                           mut timer_client: TimerClient,
                           mut spawner: impl Spawn) -> Result<(), AcceptConnectionError> 
where
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), SinkError=CSE> + Unpin + 'static,
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send,
{

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| AcceptConnectionError::RequestTimerStreamError)?;
    let opt_conn_pair = await!(connect_with_timeout(connector,
                  conn_timeout_ticks,
                  timer_stream));
    let mut conn_pair = match opt_conn_pair {
        Some(conn_pair) => Ok(conn_pair),
        None => {
            await!(pending_reject_sender.send(public_key.clone()))
                .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
            Err(AcceptConnectionError::ConnectionFailed)
        },
    }?;

    // Send first message:
    let ser_init_connection = serialize_init_connection(&InitConnection::Accept(public_key.clone()));
    let send_res = await!(conn_pair.sender.send(ser_init_connection));
    if let Err(_) = send_res {
        await!(pending_reject_sender.send(public_key))
            .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
        return Err(AcceptConnectionError::SendInitConnectionError);
    }


    let ConnPair {sender, receiver} = conn_pair;

    // Add serialization for sender:
    let to_tunnel_sender = sender
        .sink_map_err(|_| ())
        .with(|vec| -> future::Ready<Result<_,()>> {
            future::ready(Ok(serialize_tunnel_message(&vec)))
        });

    // Add deserialization for receiver:
    let from_tunnel_receiver = receiver.map(|tunnel_message| {
        deserialize_tunnel_message(&tunnel_message).ok()
    }).take_while(|opt_vec| {
        future::ready(opt_vec.is_some())
    }).map(|opt_vec| opt_vec.unwrap());


    // Deal with the keepalives:
    let (user_from_tunnel_sender, user_from_tunnel_receiver) = mpsc::channel::<Vec<u8>>(0);
    let (user_to_tunnel_sender, user_to_tunnel_receiver) = mpsc::channel::<Vec<u8>>(0);

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| AcceptConnectionError::RequestTimerStreamError)?;
    let fut_client_tunnel = client_tunnel(to_tunnel_sender, from_tunnel_receiver, 
                           user_from_tunnel_sender, user_to_tunnel_receiver,
                           timer_stream,
                           keepalive_ticks)
        .map_err(|e| error!("client_tunnel error: {:?}",e))
        .map(|_| ());

    spawner.spawn(fut_client_tunnel)
        .map_err(|_| AcceptConnectionError::SpawnError)?;

    let conn_pair = ConnPair {
        sender: user_to_tunnel_sender,
        receiver: user_from_tunnel_receiver,
    };

    await!(connections_sender.send((public_key, conn_pair)))
        .map_err(|_| AcceptConnectionError::SendConnPairError)?;
    Ok(())
}


async fn inner_client_listener<C,IAC,CS,CSE>(mut connector: C,
                                incoming_access_control: IAC,
                                connections_sender: CS,
                                conn_timeout_ticks: usize,
                                keepalive_ticks: usize,
                                mut timer_client: TimerClient,
                                mut spawner: impl Spawn + Clone + Send + 'static,
                                mut opt_event_sender: Option<mpsc::Sender<ClientListenerEvent>>) 
    -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send + Sync + Clone + 'static,
    IAC: Stream<Item=AccessControlOp> + Unpin,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), SinkError=CSE> + Unpin + Clone + Send + 'static,
    CSE: 'static
{

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_|  ClientListenerError::RequestTimerStreamError)?;

    let conn_pair = match await!(connector.connect(())) {
        Some(conn_pair) => conn_pair,
        None => return Err(ClientListenerError::ConnectionFailure),
    };

    // A channel used by the accept_connection.
    // In case of failure to accept a connection, the public key of the rejected remote host will
    // be received at pending_reject_receiver
    let (pending_reject_sender, pending_reject_receiver) = mpsc::channel::<PublicKey>(0);

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
    let receiver = receiver.map(|relay_listen_out| {
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
        .map(|access_control_op| ClientListenerEvent::AccessControlOp(access_control_op));

    let server_receiver = receiver
        .map(ClientListenerEvent::ServerMessage)
        .chain(stream::once(future::ready(ClientListenerEvent::ServerClosed)));

    let pending_reject_receiver = pending_reject_receiver
        .map(ClientListenerEvent::PendingReject);

    let mut events = timer_stream
        .select(incoming_access_control)
        .select(server_receiver)
        .select(pending_reject_receiver);

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
                        .map_err(|_| ClientListenerError::SendToServerError)?;
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            },
            ClientListenerEvent::TimerClosed => return Err(ClientListenerError::TimerClosed),
            ClientListenerEvent::AccessControlOp(access_control_op) => {
                access_control.apply_op(access_control_op)
                    .map_err(|_| ClientListenerError::AccessControlError)?;
            },
            ClientListenerEvent::ServerMessage(relay_listen_out) => {
                ticks_to_close = keepalive_ticks;
                match relay_listen_out {
                    RelayListenOut::KeepAlive => {},
                    RelayListenOut::IncomingConnection(IncomingConnection(public_key)) => {
                        if !access_control.is_allowed(&public_key) {
                            await!(sender.send(RelayListenIn::RejectConnection(RejectConnection(public_key))))
                                .map_err(|_| ClientListenerError::SendToServerError)?;
                            ticks_to_send_keepalive = keepalive_ticks / 2;
                        } else {
                            // We will attempt to accept the connection
                            let fut_accept = accept_connection(
                                public_key,
                                connector.clone(),
                                pending_reject_sender.clone(),
                                connections_sender.clone(),
                                conn_timeout_ticks,
                                keepalive_ticks,
                                timer_client.clone(),
                                spawner.clone())
                            .map_err(|e| {
                                error!("Error in accept_connection: {:?}", e);
                            }).map(|_| ());
                            spawner.spawn(fut_accept)
                                .map_err(|_| ClientListenerError::SpawnError)?;
                        }
                    }
                }
            },
            ClientListenerEvent::PendingReject(public_key) => {
                await!(sender.send(RelayListenIn::RejectConnection(RejectConnection(public_key))))
                    .map_err(|_| ClientListenerError::SendToServerError)?;
                ticks_to_send_keepalive = keepalive_ticks / 2;
            },
            ClientListenerEvent::ServerClosed => return Err(ClientListenerError::ServerClosed),
        }
    }
    Ok(())
}


/// Listen for incoming connections from a relay.
pub async fn client_listener<C,IAC,CS,CSE>(connector: C,
                                incoming_access_control: IAC,
                                connections_sender: CS,
                                conn_timeout_ticks: usize,
                                keepalive_ticks: usize,
                                timer_client: TimerClient,
                                spawner: impl Spawn + Clone + Send + 'static)
    -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    IAC: Stream<Item=AccessControlOp> + Unpin,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), SinkError=CSE> + Unpin + Clone + Send + 'static,
    CSE: 'static,
{
    await!(inner_client_listener(connector,
                                 incoming_access_control,
                                 connections_sender,
                                 conn_timeout_ticks,
                                 keepalive_ticks,
                                 timer_client,
                                 spawner,
                                 None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;



    async fn task_connect_with_timeout(spawner: impl Spawn) {
        let conn_timeout_ticks = 8;
        let (timer_sender, timer_stream) = mpsc::channel::<TimerTick>(0);
        // TODO
        /*
        connect_with_timeout(connector,
                           conn_timeout_ticks,
                           timer_stream);
                           */
    }

    #[test]
    fn test_connect_with_timeout() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_connect_with_timeout(thread_pool.clone()));
    }
}
