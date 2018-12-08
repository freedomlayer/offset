use std::marker::Unpin;

use futures::{future, FutureExt, TryFutureExt, 
    stream, Stream, StreamExt, Sink, SinkExt,
    select};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use crypto::identity::PublicKey;
use proto::relay::messages::{InitConnection, IncomingConnection, RejectConnection};
use proto::relay::serialize::{serialize_init_connection,
    deserialize_incoming_connection, serialize_reject_connection};
use common::int_convert::usize_to_u64;
use common::conn::{Listener, Connector, ConnPair, 
    ConstAddressConnector, ConnTransform};

use timer::{TimerClient, TimerTick};
use super::access_control::{AccessControl, AccessControlOp};



#[derive(Debug)]
pub enum ClientListenerError {
    RequestTimerStreamError,
    SendInitConnectionError,
    ConnectionFailure,
    TimerClosed,
    AccessControlError,
    AccessControlClosed,
    SendToServerError,
    ServerClosed,
    SpawnError,
}

#[derive(Debug, Clone)]
enum ClientListenerEvent {
    AccessControlOp(AccessControlOp),
    AccessControlClosed,
    ServerMessage(IncomingConnection),
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

async fn connect_with_timeout<C,TS>(mut connector: C,
                       conn_timeout_ticks: usize,
                       timer_stream: TS) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send,
    TS: Stream<Item=TimerTick> + Unpin,
{

    let conn_timeout_ticks = usize_to_u64(conn_timeout_ticks).unwrap();
    let mut fut_timeout = timer_stream
        .take(conn_timeout_ticks)
        .for_each(|_| future::ready(()))
        .fuse();
    let mut fut_connect = connector.connect(()).fuse();

    select! {
        _fut_timeout = fut_timeout => None,
        fut_connect = fut_connect => fut_connect,
    }
}

async fn accept_connection<C,CS,CSE,CT>(public_key: PublicKey, 
                           connector: C,
                           mut pending_reject_sender: mpsc::Sender<PublicKey>,
                           mut connections_sender: CS,
                           mut keepalive_transform: CT,
                           conn_timeout_ticks: usize,
                           mut timer_client: TimerClient) -> Result<(), AcceptConnectionError> 
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), SinkError=CSE> + Unpin + 'static,
    CT: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                      NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>,Arg=()>,
{

    let timer_stream = await!(timer_client.request_timer_stream())
        .map_err(|_| AcceptConnectionError::RequestTimerStreamError)?;
    let opt_conn_pair = await!(connect_with_timeout(connector,
                  conn_timeout_ticks,
                  timer_stream));
    let conn_pair = match opt_conn_pair {
        Some(conn_pair) => Ok(conn_pair),
        None => {
            await!(pending_reject_sender.send(public_key.clone()))
                .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
            Err(AcceptConnectionError::ConnectionFailed)
        },
    }?;

    let (mut sender, receiver) = conn_pair;

    // Send first message:
    let ser_init_connection = serialize_init_connection(
        &InitConnection::Accept(public_key.clone()));
    let send_res = await!(sender.send(ser_init_connection));
    if let Err(_) = send_res {
        await!(pending_reject_sender.send(public_key))
            .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
        return Err(AcceptConnectionError::SendInitConnectionError);
    }

    let to_tunnel_sender = sender;
    let from_tunnel_receiver = receiver;

    let (user_to_tunnel_sender, user_from_tunnel_receiver) = 
        await!(keepalive_transform.transform((), (to_tunnel_sender, from_tunnel_receiver))).unwrap();

    await!(connections_sender.send((public_key, (user_to_tunnel_sender, user_from_tunnel_receiver))))
        .map_err(|_| AcceptConnectionError::SendConnPairError)?;
    Ok(())
}



async fn inner_client_listener<'a, C,IAC,CS,CSE,CT>(mut connector: C,
                                access_control: &'a mut AccessControl,
                                incoming_access_control: &'a mut IAC,
                                connections_sender: CS,
                                keepalive_transform: CT,
                                conn_timeout_ticks: usize,
                                timer_client: TimerClient,
                                mut spawner: impl Spawn + Clone + Send + 'static,
                                mut opt_event_sender: Option<mpsc::Sender<ClientListenerEvent>>) 
    -> Result<(), ClientListenerError>
where
    C: Connector<Address=(), SendItem=Vec<u8>, RecvItem=Vec<u8>> + Send + Sync + Clone + 'static,
    IAC: Stream<Item=AccessControlOp> + Unpin + 'static,
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>), SinkError=CSE> + Unpin + Clone + Send + 'static,
    CSE: 'static,
    CT: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                      NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>,Arg=()> + Send + Clone + 'static,
{
    let conn_pair = match await!(connector.connect(())) {
        Some(conn_pair) => conn_pair,
        None => return Err(ClientListenerError::ConnectionFailure),
    };

    // A channel used by the accept_connection.
    // In case of failure to accept a connection, the public key of the rejected remote host will
    // be received at pending_reject_receiver
    let (pending_reject_sender, pending_reject_receiver) = mpsc::channel::<PublicKey>(0);

    let (mut sender, receiver) = conn_pair;
    let ser_init_connection = serialize_init_connection(&InitConnection::Listen);

    await!(sender.send(ser_init_connection))
        .map_err(|_| ClientListenerError::SendInitConnectionError)?;

    // Add serialization for sender:
    let mut sender = sender
        .sink_map_err(|_| ())
        .with(|vec| -> future::Ready<Result<_,()>> {
            future::ready(Ok(serialize_reject_connection(&vec)))
        });

    // Add deserialization for receiver:
    let receiver = receiver.map(|ser_incoming_connection| {
        deserialize_incoming_connection(&ser_incoming_connection).ok()
    }).take_while(|opt_incoming_connection| {
        future::ready(opt_incoming_connection.is_some())
    }).map(|opt| opt.unwrap());


    let incoming_access_control = incoming_access_control
        .map(|access_control_op| ClientListenerEvent::AccessControlOp(access_control_op))
        .chain(stream::once(future::ready(ClientListenerEvent::AccessControlClosed)));

    let server_receiver = receiver
        .map(ClientListenerEvent::ServerMessage)
        .chain(stream::once(future::ready(ClientListenerEvent::ServerClosed)));

    let pending_reject_receiver = pending_reject_receiver
        .map(ClientListenerEvent::PendingReject);

    let mut events = incoming_access_control
        .select(server_receiver)
        .select(pending_reject_receiver);

    while let Some(event) = await!(events.next()) {
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = await!(event_sender.send(event.clone()));
        }
        match event {
            ClientListenerEvent::AccessControlOp(access_control_op) =>
                access_control.apply_op(access_control_op),
            ClientListenerEvent::ServerMessage(incoming_connection) => {
                let public_key = incoming_connection.public_key.clone();
                if !access_control.is_allowed(&public_key) {
                    await!(sender.send(RejectConnection { public_key }))
                        .map_err(|_| ClientListenerError::SendToServerError)?;
                } else {
                    // We will attempt to accept the connection
                    let fut_accept = accept_connection(
                        public_key,
                        connector.clone(),
                        pending_reject_sender.clone(),
                        connections_sender.clone(),
                        keepalive_transform.clone(),
                        conn_timeout_ticks,
                        timer_client.clone())
                    .map_err(|e| {
                        error!("Error in accept_connection: {:?}", e);
                    }).map(|_| ());
                    spawner.spawn(fut_accept)
                        .map_err(|_| ClientListenerError::SpawnError)?;
                }
            },
            ClientListenerEvent::PendingReject(public_key) => {
                await!(sender.send(RejectConnection { public_key }))
                    .map_err(|_| ClientListenerError::SendToServerError)?;
            },
            ClientListenerEvent::ServerClosed => return Err(ClientListenerError::ServerClosed),
            ClientListenerEvent::AccessControlClosed =>
                return Err(ClientListenerError::AccessControlClosed),
        }
    }
    Ok(())
}


#[derive(Clone)]
pub struct ClientListener<C,CT,S> {
    connector: C,
    access_control: AccessControl,
    keepalive_transform: CT,
    conn_timeout_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
}

impl<C,CT,S> ClientListener<C,CT,S> {
    pub fn new(connector: C,
           access_control: AccessControl,
           keepalive_transform: CT,
           conn_timeout_ticks: usize,
           timer_client: TimerClient,
           spawner: S) -> ClientListener<C,CT,S> {

        ClientListener {
            connector,
            access_control,
            keepalive_transform,
            conn_timeout_ticks,
            timer_client,
            spawner,
        }
    }
}

impl <A,C,CT,S> Listener for ClientListener<C,CT,S> 
where
    A: Clone + Send + Sync + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    S: Spawn + Clone + Send + 'static,
    CT: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                      NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>,Arg=()> + Clone + Send + 'static,
{
    type Connection = (PublicKey, ConnPair<Vec<u8>, Vec<u8>>);
    type Config = AccessControlOp;
    type Arg = (A, AccessControl);

    fn listen(self, arg: (A, AccessControl)) -> (mpsc::Sender<AccessControlOp>, 
                             mpsc::Receiver<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>) {

        let (relay_address, mut access_control) = arg;

        let mut c_spawner = self.spawner.clone();
        let (access_control_sender, mut access_control_receiver) = mpsc::channel(0);
        let (connections_sender, connections_receiver) = mpsc::channel(0);

        let const_connector = ConstAddressConnector::new(
            self.connector.clone(),
            relay_address);

        let fut = async move {
            await!(inner_client_listener(const_connector,
                                 &mut access_control,
                                 &mut access_control_receiver,
                                 connections_sender,
                                 self.keepalive_transform,
                                 self.conn_timeout_ticks,
                                 self.timer_client,
                                 self.spawner,
                                 None)
                        .map_err(|e| error!("inner_client_listener() error: {:?}", e))
                        .map(|_| ()))
        };

        let _ = c_spawner.spawn(fut);

        (access_control_sender, connections_receiver)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::channel::oneshot;
    use proto::relay::serialize::{deserialize_init_connection};
    use crypto::identity::PUBLIC_KEY_LEN;
    use timer::create_timer_incoming;

    use proto::relay::serialize::{serialize_incoming_connection, 
        deserialize_reject_connection};

    use common::dummy_connector::DummyConnector;
    use common::conn::IdentityConnTransform;

    async fn task_connect_with_timeout_basic(mut spawner: impl Spawn) {
        let conn_timeout_ticks = 8;
        let (_timer_sender, timer_stream) = mpsc::channel::<TimerTick>(0);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);

        let fut_connect = connect_with_timeout(connector,
                             conn_timeout_ticks,
                             timer_stream);
        let fut_conn = spawner.spawn_with_handle(fut_connect).unwrap();

        let req = await!(req_receiver.next()).unwrap();
        let (dummy_sender, dummy_receiver) = mpsc::channel::<Vec<u8>>(0);
        let conn_pair = (dummy_sender, dummy_receiver);
        req.reply(Some(conn_pair));


        assert!(await!(fut_conn).is_some());
    }

    #[test]
    fn test_connect_with_timeout_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_connect_with_timeout_basic(thread_pool.clone()));
    }

    async fn task_connect_with_timeout_timeout(mut spawner: impl Spawn) {
        let conn_timeout_ticks = 8;
        let (mut timer_sender, timer_stream) = mpsc::channel::<TimerTick>(0);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);

        let (res_sender, res_receiver) = oneshot::channel();
        
        spawner.spawn(async move {
            let res = await!(connect_with_timeout(connector,
                                conn_timeout_ticks,
                                timer_stream));
            res_sender.send(res).unwrap();
        }).unwrap();

        let req = await!(req_receiver.next()).unwrap();
        assert_eq!(req.address, ());

        for _ in 0 .. 8usize { 
            await!(timer_sender.send(TimerTick)).unwrap();
        }

        assert!(await!(res_receiver).unwrap().is_none());

    }

    #[test]
    fn test_connect_with_timeout_timeout() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_connect_with_timeout_timeout(thread_pool.clone()));
    }


    async fn task_accept_connection_basic(mut spawner: impl Spawn + Clone + Send + 'static) {
        let public_key = PublicKey::from(&[0x77; PUBLIC_KEY_LEN]);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);
        let (pending_reject_sender, _pending_reject_receiver) = mpsc::channel(0);
        let (connections_sender, mut connections_receiver) = mpsc::channel(0);
        let conn_timeout_ticks = 8;
        let (_tick_sender, tick_receiver) = mpsc::channel(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone())
            .unwrap();

        // We don't need a real keepalive transform for this test:
        let keepalive_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,()>::new();

        let fut_accept = accept_connection(public_key.clone(),
                           connector,
                           pending_reject_sender,
                           connections_sender,
                           keepalive_transform,
                           conn_timeout_ticks,
                           timer_client)
            .map_err(|e| error!("accept_connection error: {:?}", e))
            .map(|_| ());

        spawner.spawn(fut_accept).unwrap();

        let (local_sender, mut remote_receiver) = mpsc::channel(0);
        let (remote_sender, local_receiver) = mpsc::channel(0);

        let conn_pair = (local_sender, local_receiver);

        // accept_connection() will try to connect. We prepare a connection:
        let req = await!(req_receiver.next()).unwrap();
        req.reply(Some(conn_pair));

        let vec_init_connection = await!(remote_receiver.next()).unwrap();
        let init_connection = deserialize_init_connection(&vec_init_connection).unwrap();
        if let InitConnection::Accept(accept_public_key) = init_connection {
            assert_eq!(accept_public_key, public_key);
        } else {
            unreachable!();
        }

        let mut ser_remote_sender = remote_sender;
        let mut ser_remote_receiver = remote_receiver;

        let (accepted_public_key, conn_pair) = await!(connections_receiver.next()).unwrap();
        assert_eq!(accepted_public_key, public_key);

        let (mut sender, mut receiver) = conn_pair;

        await!(sender.send(vec![1,2,3])).unwrap();
        let res = await!(ser_remote_receiver.next()).unwrap();
        assert_eq!(res, vec![1,2,3]);

        await!(ser_remote_sender.send(vec![3,2,1])).unwrap();
        let res = await!(receiver.next()).unwrap();
        assert_eq!(res, vec![3,2,1]);
    }

    #[test]
    fn test_accept_connection_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_accept_connection_basic(thread_pool.clone()));
    }

    async fn task_client_listener_basic(mut spawner: impl Spawn + Clone + Send + 'static) {
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);
        let (connections_sender, _connections_receiver) = mpsc::channel(0);
        let conn_timeout_ticks = 8;
        let (_tick_sender, tick_receiver) = mpsc::channel(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone())
            .unwrap();

        let (mut acl_sender, mut incoming_access_control) = mpsc::channel(0);
        let (event_sender, mut event_receiver) = mpsc::channel(0);
        let keepalive_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,()>::new();

        let c_spawner = spawner.clone();
        let fut_listener = async move {
            let mut access_control = AccessControl::new();
            await!(inner_client_listener(connector,
                              &mut access_control,
                              &mut incoming_access_control,
                              connections_sender,
                              keepalive_transform,
                              conn_timeout_ticks,
                              timer_client,
                              c_spawner,
                              Some(event_sender)))
        }.map_err(|e| error!("inner_client_listener error: {:?}",e))
        .map(|_| ());

        spawner.spawn(fut_listener).unwrap();

        // listener will attempt to start a main connection to the relay:
        let (mut relay_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, mut relay_receiver) = mpsc::channel(0);
        let conn_pair = (local_sender, local_receiver);
        let req = await!(req_receiver.next()).unwrap();
        req.reply(Some(conn_pair));

        // Open access for a certain public key:
        let public_key_a = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        await!(acl_sender.send(AccessControlOp::Add(public_key_a.clone()))).unwrap();
        await!(event_receiver.next()).unwrap();

        // First message to the relay should be InitConnection::Listen:
        let vec_init_connection = await!(relay_receiver.next()).unwrap();
        let init_connection = deserialize_init_connection(&vec_init_connection).unwrap();
        if let InitConnection::Listen = init_connection {
        } else {
            unreachable!();
        }

        // Relay will now send a message about incoming connection from a public key that is not
        // allowed:
        let public_key_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let relay_listen_out = IncomingConnection { public_key: public_key_b.clone() };
        let vec_incoming_connection = serialize_incoming_connection(&relay_listen_out);
        await!(relay_sender.send(vec_incoming_connection)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Listener will reject the connection:
        let vec_relay_listen_in = await!(relay_receiver.next()).unwrap();
        let reject_connection = deserialize_reject_connection(&vec_relay_listen_in).unwrap();
        assert_eq!(reject_connection.public_key, public_key_b);

        // Relay will now send a message about incoming connection from a public key that is
        // allowed:
        let incoming_connection = IncomingConnection { public_key: public_key_a.clone() };
        let vec_incoming_connection = serialize_incoming_connection(&incoming_connection);
        await!(relay_sender.send(vec_incoming_connection)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Listener will accept the connection:
        // Listener will open a connection to the relay:
        let (_remote_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, mut remote_receiver) = mpsc::channel(0);
        let conn_pair = (local_sender, local_receiver);

        let req = await!(req_receiver.next()).unwrap();
        req.reply(Some(conn_pair));

        let vec_init_connection = await!(remote_receiver.next()).unwrap();
        let init_connection = deserialize_init_connection(&vec_init_connection).unwrap();
        if let InitConnection::Accept(accepted_public_key) = init_connection {
            assert_eq!(accepted_public_key, public_key_a);
        } else {
            unreachable!();
        }
    }


    #[test]
    fn test_client_listener_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_client_listener_basic(thread_pool.clone()));
    }


    // TODO: Add a test for ClientListener.

}
