use std::marker::Unpin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, select, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{
    BoxFuture, BoxStream, ConnPairVec, ConstFutTransform, FutTransform, Listener, ListenerClient,
};

use proto::crypto::PublicKey;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};
use proto::relay::messages::{IncomingConnection, InitConnection, RejectConnection};

use common::access_control::{AccessControl, AccessControlOp};
use common::select_streams::select_streams;

use timer::{TimerClient, TimerTick};

type AccessControlPk = AccessControl<PublicKey>;
type AccessControlOpPk = AccessControlOp<PublicKey>;

#[derive(Debug)]
pub enum ClientListenerError {
    SendInitConnectionError,
    ConnectionFailure,
    // AccessControlClosed,
    SendToServerError,
    // ServerClosed,
    SpawnError,
}

#[derive(Debug, Clone)]
enum ClientListenerEvent {
    AccessControlOp(AccessControlOpPk),
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
}

/*
/// Convert a Sink to an mpsc::Sender<T>
/// This is done to overcome some compiler type limitations.
fn to_mpsc_sender<T,SI,SE>(mut sink: SI, spawner: impl Spawn) -> mpsc::Sender<T>
where
    SI: Sink<T, Error=SE> + Unpin + Send + 'static,
    T: Send + 'static,
{
    let (sender, mut receiver) = mpsc::channel::<T>(0);
    let fut = async move {
        sink.send_all(&mut receiver).await
    }.map(|_| ());
    spawner.spawn(fut).unwrap();
    sender
}

/// Convert a Stream to an mpsc::Receiver<T>
/// This is done to overcome some compiler type limitations.
fn to_mpsc_receiver<T,ST,SE>(mut stream: ST, spawner: impl Spawn) -> mpsc::Receiver<T>
where
    ST: Stream<Item=T> + Unpin + Send + 'static,
    T: Send + 'static,
{
    let (mut sender, receiver) = mpsc::channel::<T>(0);
    let fut = async move {
        sender.send_all(&mut stream).await
    }.map(|_| ());
    spawner.spawn(fut).unwrap();
    receiver
}
*/

async fn connect_with_timeout<C, TS>(
    mut connector: C,
    conn_timeout_ticks: usize,
    timer_stream: TS,
) -> Option<ConnPairVec>
where
    C: FutTransform<Input = (), Output = Option<ConnPairVec>> + Send,
    TS: Stream<Item = TimerTick> + Unpin,
{
    let fut_timeout = timer_stream
        .take(conn_timeout_ticks)
        .for_each(|_| future::ready(()));
    let fut_connect = connector.transform(());

    select! {
        _fut_timeout = fut_timeout.fuse() => {
            warn!("connection_with_timeout(): Timeout occurred during connection attempt");
            None
        },
        fut_connect = fut_connect.fuse() => fut_connect,
    }
}

async fn accept_connection<C, CS, CSE>(
    public_key: PublicKey,
    connector: C,
    mut pending_reject_sender: mpsc::Sender<PublicKey>,
    mut connections_sender: CS,
    conn_timeout_ticks: usize,
    mut timer_client: TimerClient,
) -> Result<(), AcceptConnectionError>
where
    C: FutTransform<Input = (), Output = Option<ConnPairVec>> + Send,
    CS: Sink<(PublicKey, ConnPairVec), Error = CSE> + Unpin + 'static,
{
    let timer_stream = timer_client
        .request_timer_stream("accept_connection".to_owned())
        .await
        .map_err(|_| AcceptConnectionError::RequestTimerStreamError)?;
    let opt_conn_pair = connect_with_timeout(connector, conn_timeout_ticks, timer_stream).await;
    let conn_pair = match opt_conn_pair {
        Some(conn_pair) => Ok(conn_pair),
        None => {
            pending_reject_sender
                .send(public_key.clone())
                .await
                .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
            Err(AcceptConnectionError::ConnectionFailed)
        }
    }?;

    let (mut sender, receiver) = conn_pair.split();

    // Send first message:
    let ser_init_connection = InitConnection::Accept(public_key.clone()).proto_serialize();
    let send_res = sender.send(ser_init_connection).await;
    if send_res.is_err() {
        pending_reject_sender
            .send(public_key)
            .await
            .map_err(|_| AcceptConnectionError::PendingRejectSenderError)?;
        return Err(AcceptConnectionError::SendInitConnectionError);
    }

    let to_tunnel_sender = sender;
    let from_tunnel_receiver = receiver;

    connections_sender
        .send((
            public_key,
            ConnPairVec::from_raw(to_tunnel_sender, from_tunnel_receiver),
        ))
        .await
        .map_err(|_| AcceptConnectionError::SendConnPairError)?;
    Ok(())
}

async fn inner_client_listener<'a, C, IAC, CS, CSE>(
    mut connector: C,
    access_control: &'a mut AccessControlPk,
    incoming_access_control: &'a mut IAC,
    connections_sender: CS,
    conn_timeout_ticks: usize,
    timer_client: TimerClient,
    spawner: impl Spawn + Clone + Send + 'static,
    mut opt_event_sender: Option<mpsc::Sender<ClientListenerEvent>>,
) -> Result<(), ClientListenerError>
where
    C: FutTransform<Input = (), Output = Option<ConnPairVec>> + Send + Clone + 'static,
    IAC: Stream<Item = AccessControlOp<PublicKey>> + Unpin + Send + 'static,
    CS: Sink<(PublicKey, ConnPairVec), Error = CSE> + Unpin + Clone + Send + 'static,
    CSE: 'static,
{
    let conn_pair = match connector.transform(()).await {
        Some(conn_pair) => conn_pair,
        None => return Err(ClientListenerError::ConnectionFailure),
    };

    // A channel used by the accept_connection.
    // In case of failure to accept a connection, the public key of the rejected remote host will
    // be received at pending_reject_receiver
    let (pending_reject_sender, pending_reject_receiver) = mpsc::channel::<PublicKey>(0);

    let (mut sender, receiver) = conn_pair.split();
    let ser_init_connection = InitConnection::Listen.proto_serialize();

    sender
        .send(ser_init_connection)
        .await
        .map_err(|_| ClientListenerError::SendInitConnectionError)?;

    // Add serialization for sender:
    let mut sender =
        sender
            .sink_map_err(|_| ())
            .with(|vec: RejectConnection| -> future::Ready<Result<_, ()>> {
                future::ready(Ok(vec.proto_serialize()))
            });

    // Add deserialization for receiver:
    let receiver = receiver
        .map(|ser_incoming_connection| {
            match IncomingConnection::proto_deserialize(&ser_incoming_connection) {
                Ok(incoming_connection) => Some(incoming_connection),
                Err(e) => {
                    error!("Error deserializing incoming connection {:?}", e);
                    None
                }
            }
        })
        .take_while(|opt_incoming_connection| future::ready(opt_incoming_connection.is_some()))
        .map(Option::unwrap);

    let incoming_access_control = incoming_access_control
        .map(ClientListenerEvent::AccessControlOp)
        .chain(stream::once(future::ready(
            ClientListenerEvent::AccessControlClosed,
        )));

    let server_receiver = receiver
        .map(ClientListenerEvent::ServerMessage)
        .chain(stream::once(future::ready(
            ClientListenerEvent::ServerClosed,
        )));

    let pending_reject_receiver = pending_reject_receiver.map(ClientListenerEvent::PendingReject);

    let mut events = select_streams![
        incoming_access_control,
        server_receiver,
        pending_reject_receiver
    ];

    while let Some(event) = events.next().await {
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(event.clone()).await;
        }
        match event {
            ClientListenerEvent::AccessControlOp(access_control_op) => {
                access_control.apply_op(access_control_op)
            }
            ClientListenerEvent::ServerMessage(incoming_connection) => {
                let public_key = incoming_connection.public_key.clone();
                if !access_control.is_allowed(&public_key) {
                    sender
                        .send(RejectConnection { public_key })
                        .await
                        .map_err(|_| ClientListenerError::SendToServerError)?;
                } else {
                    // We will attempt to accept the connection
                    let fut_accept = accept_connection(
                        public_key,
                        connector.clone(),
                        pending_reject_sender.clone(),
                        connections_sender.clone(),
                        conn_timeout_ticks,
                        timer_client.clone(),
                    )
                    .map_err(|e| {
                        error!("Error in accept_connection: {:?}", e);
                    })
                    .map(|_| ());
                    spawner
                        .spawn(fut_accept)
                        .map_err(|_| ClientListenerError::SpawnError)?;
                }
            }
            ClientListenerEvent::PendingReject(public_key) => {
                sender
                    .send(RejectConnection { public_key })
                    .await
                    .map_err(|_| ClientListenerError::SendToServerError)?;
            }
            ClientListenerEvent::ServerClosed => break,
            ClientListenerEvent::AccessControlClosed => break,
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct ClientListener<C, S> {
    connector: C,
    /// Amount of ticks to wait before we give up on connecting to a relay
    conn_timeout_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
}

impl<C, S> ClientListener<C, S> {
    pub fn new(
        connector: C,
        conn_timeout_ticks: usize,
        timer_client: TimerClient,
        spawner: S,
    ) -> ClientListener<C, S> {
        ClientListener {
            connector,
            conn_timeout_ticks,
            timer_client,
            spawner,
        }
    }
}

impl<A, C, S> Listener for ClientListener<C, S>
where
    A: Clone + Send + 'static,
    C: FutTransform<Input = A, Output = Option<ConnPairVec>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Connection = (PublicKey, ConnPairVec);
    type Config = AccessControlOpPk;
    type Error = ClientListenerError;
    type Arg = (A, AccessControlPk);

    fn listen(
        self,
        arg: Self::Arg,
    ) -> BoxFuture<'static, Result<ListenerClient<Self::Config, Self::Connection>, Self::Error>>
    {
        let (relay_address, mut access_control) = arg;

        let c_spawner = self.spawner.clone();
        let (access_control_sender, mut access_control_receiver) = mpsc::channel(0);
        let (connections_sender, connections_receiver) = mpsc::channel(0);

        let const_connector = ConstFutTransform::new(self.connector.clone(), relay_address);

        // TODO: We currently spawn too much logic here.
        // We should be able to perform block on the connection part inside listen invocation,
        // and spawn the rest of the logic later:
        let fut = async move {
            inner_client_listener(
                const_connector,
                &mut access_control,
                &mut access_control_receiver,
                connections_sender,
                self.conn_timeout_ticks,
                self.timer_client,
                self.spawner,
                None,
            )
            .map_err(|e| warn!("inner_client_listener() error: {:?}", e))
            .map(|_| ())
            .await
        };

        let _ = c_spawner.spawn(fut);

        Box::pin(future::ready(Ok(ListenerClient {
            config_sender: access_control_sender,
            conn_receiver: connections_receiver,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::oneshot;
    use futures::executor::{LocalPool, ThreadPool};
    use timer::create_timer_incoming;

    use common::dummy_connector::DummyConnector;

    async fn task_connect_with_timeout_basic(spawner: impl Spawn) {
        let conn_timeout_ticks = 8;
        let (_timer_sender, timer_stream) = mpsc::channel::<TimerTick>(0);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);

        let fut_connect = connect_with_timeout(connector, conn_timeout_ticks, timer_stream);
        let fut_conn = spawner.spawn_with_handle(fut_connect).unwrap();

        let req = req_receiver.next().await.unwrap();
        let (dummy_sender, dummy_receiver) = mpsc::channel::<Vec<u8>>(0);
        let conn_pair = ConnPairVec::from_raw(dummy_sender, dummy_receiver);
        req.reply(Some(conn_pair));

        assert!(fut_conn.await.is_some());
    }

    #[test]
    fn test_connect_with_timeout_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_connect_with_timeout_basic(thread_pool.clone()));
    }

    async fn task_connect_with_timeout_timeout(spawner: impl Spawn) {
        let conn_timeout_ticks = 8;
        let (mut timer_sender, timer_stream) = mpsc::channel::<TimerTick>(0);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);

        let (res_sender, res_receiver) = oneshot::channel();

        spawner
            .spawn(async move {
                let res = connect_with_timeout(connector, conn_timeout_ticks, timer_stream).await;
                res_sender.send(res).unwrap();
            })
            .unwrap();

        let req = req_receiver.next().await.unwrap();
        assert_eq!(req.address, ());

        for _ in 0..8usize {
            timer_sender.send(TimerTick).await.unwrap();
        }

        assert!(res_receiver.await.unwrap().is_none());
    }

    #[test]
    fn test_connect_with_timeout_timeout() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_connect_with_timeout_timeout(thread_pool.clone()));
    }

    async fn task_accept_connection_basic(spawner: impl Spawn + Clone + Send + 'static) {
        let public_key = PublicKey::from(&[0x77; PublicKey::len()]);
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);
        let (pending_reject_sender, _pending_reject_receiver) = mpsc::channel(0);
        let (connections_sender, mut connections_receiver) = mpsc::channel(0);
        let conn_timeout_ticks = 8;
        let (_tick_sender, tick_receiver) = mpsc::channel(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let fut_accept = accept_connection(
            public_key.clone(),
            connector,
            pending_reject_sender,
            connections_sender,
            conn_timeout_ticks,
            timer_client,
        )
        .map_err(|e| error!("accept_connection error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_accept).unwrap();

        let (local_sender, mut remote_receiver) = mpsc::channel(1);
        let (remote_sender, local_receiver) = mpsc::channel(1);

        let conn_pair = ConnPairVec::from_raw(local_sender, local_receiver);

        // accept_connection() will try to connect. We prepare a connection:
        let req = req_receiver.next().await.unwrap();
        req.reply(Some(conn_pair));

        let vec_init_connection = remote_receiver.next().await.unwrap();
        let init_connection = InitConnection::proto_deserialize(&vec_init_connection).unwrap();
        if let InitConnection::Accept(accept_public_key) = init_connection {
            assert_eq!(accept_public_key, public_key);
        } else {
            unreachable!();
        }

        let mut ser_remote_sender = remote_sender;
        let mut ser_remote_receiver = remote_receiver;

        let (accepted_public_key, conn_pair) = connections_receiver.next().await.unwrap();
        assert_eq!(accepted_public_key, public_key);

        let (mut sender, mut receiver) = conn_pair.split();

        sender.send(vec![1, 2, 3]).await.unwrap();
        let res = ser_remote_receiver.next().await.unwrap();
        assert_eq!(res, vec![1, 2, 3]);

        ser_remote_sender.send(vec![3, 2, 1]).await.unwrap();
        let res = receiver.next().await.unwrap();
        assert_eq!(res, vec![3, 2, 1]);
    }

    #[test]
    fn test_accept_connection_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_accept_connection_basic(thread_pool.clone()));
    }

    async fn task_client_listener_basic(spawner: impl Spawn + Clone + Send + 'static) {
        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let connector = DummyConnector::new(req_sender);
        let (connections_sender, _connections_receiver) = mpsc::channel(0);
        let conn_timeout_ticks = 8;
        let (_tick_sender, tick_receiver) = mpsc::channel(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (mut acl_sender, mut incoming_access_control) = mpsc::channel(0);
        let (event_sender, mut event_receiver) = mpsc::channel(0);

        let c_spawner = spawner.clone();
        let fut_listener = async move {
            let mut access_control = AccessControlPk::new();
            // HACK: Saving temp value in res to make the compiler happy.
            let res = inner_client_listener(
                connector,
                &mut access_control,
                &mut incoming_access_control,
                connections_sender,
                conn_timeout_ticks,
                timer_client,
                c_spawner,
                Some(event_sender),
            )
            .await;
            res
        }
        .map_err(|e| warn!("inner_client_listener error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_listener).unwrap();

        // listener will attempt to start a main connection to the relay:
        let (mut relay_sender, local_receiver) = mpsc::channel(1);
        let (local_sender, mut relay_receiver) = mpsc::channel(1);
        let conn_pair = ConnPairVec::from_raw(local_sender, local_receiver);
        let req = req_receiver.next().await.unwrap();
        req.reply(Some(conn_pair));

        // Open access for a certain public key:
        let public_key_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        acl_sender
            .send(AccessControlOp::Add(public_key_a.clone()))
            .await
            .unwrap();
        event_receiver.next().await.unwrap();

        // First message to the relay should be InitConnection::Listen:
        let vec_init_connection = relay_receiver.next().await.unwrap();
        let init_connection = InitConnection::proto_deserialize(&vec_init_connection).unwrap();
        if let InitConnection::Listen = init_connection {
        } else {
            unreachable!();
        }

        // Relay will now send a message about incoming connection from a public key that is not
        // allowed:
        let public_key_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let relay_listen_out = IncomingConnection {
            public_key: public_key_b.clone(),
        };
        let vec_incoming_connection = relay_listen_out.proto_serialize();
        relay_sender.send(vec_incoming_connection).await.unwrap();
        event_receiver.next().await.unwrap();

        // Listener will reject the connection:
        let vec_relay_listen_in = relay_receiver.next().await.unwrap();
        let reject_connection = RejectConnection::proto_deserialize(&vec_relay_listen_in).unwrap();
        assert_eq!(reject_connection.public_key, public_key_b);

        // Relay will now send a message about incoming connection from a public key that is
        // allowed:
        let incoming_connection = IncomingConnection {
            public_key: public_key_a.clone(),
        };
        let vec_incoming_connection = incoming_connection.proto_serialize();
        relay_sender.send(vec_incoming_connection).await.unwrap();
        event_receiver.next().await.unwrap();

        // Listener will accept the connection:
        // Listener will open a connection to the relay:
        let (_remote_sender, local_receiver) = mpsc::channel(0);
        let (local_sender, mut remote_receiver) = mpsc::channel(0);
        let conn_pair = ConnPairVec::from_raw(local_sender, local_receiver);

        let req = req_receiver.next().await.unwrap();
        req.reply(Some(conn_pair));

        let vec_init_connection = remote_receiver.next().await.unwrap();
        let init_connection = InitConnection::proto_deserialize(&vec_init_connection).unwrap();
        if let InitConnection::Accept(accepted_public_key) = init_connection {
            assert_eq!(accepted_public_key, public_key_a);
        } else {
            unreachable!();
        }
    }

    #[test]
    fn test_client_listener_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_client_listener_basic(thread_pool.clone()));
    }
    // TODO: Add a test for ClientListener.
}
