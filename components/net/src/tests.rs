use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bytes::Bytes;

use env_logger;

use futures::executor::ThreadPool;
use futures::task::Spawn;
use futures::{FutureExt, SinkExt, StreamExt};
use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures_01::stream::Stream as Stream01;
use futures_01::sink::Sink as Sink01;


use common::conn::{FutTransform, Listener};
use proto::net::messages::NetAddress;

use crate::net_connector::NetConnector;
use crate::tcp_connector::TcpConnector;
use crate::tcp_listener::TcpListener;

use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream;
use tokio::codec::{Framed, LengthDelimitedCodec};

/// Get an available port we can listen on
fn get_available_port_v4() -> u16 {
    // Idea based on code at:
    // https://github.com/rust-lang-nursery/rust-cookbook/pull/137/files
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    // Assigning port 0 requests the OS to assign a free port
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 0);
    let listener = TokioTcpListener::bind(&socket_addr).unwrap();
    listener.local_addr().unwrap().port()
}

const TEST_MAX_FRAME_LEN: usize = 0x100;

async fn task_tcp_client_server_v4<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let available_port = get_available_port_v4();
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut tcp_connector = TcpConnector::new(TEST_MAX_FRAME_LEN, spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    for _ in 0..5 {
        let (mut client_sender, mut client_receiver) =
            await!(tcp_connector.transform(socket_addr.clone())).unwrap();
        let (mut server_sender, mut server_receiver) = await!(incoming_connections.next()).unwrap();

        await!(client_sender.send(vec![1, 2, 3])).unwrap();
        assert_eq!(await!(server_receiver.next()).unwrap(), vec![1, 2, 3]);

        await!(server_sender.send(vec![3, 2, 1])).unwrap();
        assert_eq!(await!(client_receiver.next()).unwrap(), vec![3, 2, 1]);
    }

    /*
    // Dropping incoming_connections should close the listener after a while
    drop(incoming_connections);

    // TODO: Do we want the tcp_listener to be closed immediately when incoming_connections is
    // dropped? Is this possible?
    for _ in 0 .. 5 {
        await!(tcp_connector.transform(socket_addr.clone()));
    }
    assert!(await!(tcp_connector.transform(socket_addr.clone())).is_none());
    */
}

#[test]
fn test_tcp_client_server_v4() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_tcp_client_server_v4(thread_pool.clone()));
}

async fn task_net_connector_v4_basic<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let available_port = get_available_port_v4();
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut net_connector = NetConnector::new(TEST_MAX_FRAME_LEN, spawner.clone(), spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    let net_address: NetAddress = format!("127.0.0.1:{}", available_port).try_into().unwrap();

    for _ in 0..5 {
        let (mut client_sender, mut client_receiver) =
            await!(net_connector.transform(net_address.clone())).unwrap();
        let (mut server_sender, mut server_receiver) = await!(incoming_connections.next()).unwrap();

        await!(client_sender.send(vec![1, 2, 3])).unwrap();
        assert_eq!(await!(server_receiver.next()).unwrap(), vec![1, 2, 3]);

        await!(server_sender.send(vec![3, 2, 1])).unwrap();
        assert_eq!(await!(client_receiver.next()).unwrap(), vec![3, 2, 1]);
    }
}

#[test]
fn test_net_connector_v4_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_net_connector_v4_basic(thread_pool.clone()));
}


async fn task_net_connector_v4_drop_sender<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{

    let available_port = get_available_port_v4();
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut net_connector = NetConnector::new(TEST_MAX_FRAME_LEN, spawner.clone(), spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    let net_address: NetAddress = format!("127.0.0.1:{}", available_port).try_into().unwrap();

    let (client_sender, _client_receiver) =
        await!(net_connector.transform(net_address.clone())).unwrap();
    let (_server_sender, mut server_receiver) = await!(incoming_connections.next()).unwrap();

    // Drop the client's sender:
    drop(client_sender);

    // Wait until the server understands the connection is closed.
    // This should happen quickly.
    while let Some(_) = await!(server_receiver.next()) {
    }
}

#[test]
fn test_net_connector_v4_drop_sender() {
    env_logger::init();
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_net_connector_v4_drop_sender(thread_pool.clone()));
}


fn tcp_stream_to_conn_pair_01(
    tcp_stream: TcpStream,
    max_frame_length: usize,
) -> (impl Sink01<SinkItem=Vec<u8>>, impl Stream01<Item=Vec<u8>>)
{
    let mut codec = LengthDelimitedCodec::new();
    codec.set_max_frame_length(max_frame_length);
    let (sender_01, receiver_01) = Framed::new(tcp_stream, codec).split();

    // Conversion layer between Vec<u8> to Bytes:
    let sender_01 = sender_01
        .sink_map_err(|_| {
            info!("tcp_stream_to_conn_pair(): sender_01 error!");
            ()
        })
        .with(|vec: Vec<u8>| -> Result<Bytes, ()> { Ok(Bytes::from(vec)) });

    let receiver_01 = receiver_01.map(|bytes| bytes.to_vec());

    (sender_01, receiver_01)
}


async fn task_tcp_stream_to_conn_pair_drop_sender<S>(_spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{

    let max_frame_length = 0x100;

    let available_port = get_available_port_v4();
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    // Set up server side (listen):
    let listener = TokioTcpListener::bind(&socket_addr).unwrap();

    let mut incoming_conns = listener.incoming().compat();
    let fut_server_tcp_stream = incoming_conns.next();

    // Set up client side (connect):
    let fut_client_tcp_stream = TcpStream::connect(&socket_addr).compat();

    let (opt_server_tcp_stream, opt_client_tcp_stream) = 
        await!(fut_server_tcp_stream
        .join(fut_client_tcp_stream));

    let (_server_sender, server_receiver) = tcp_stream_to_conn_pair_01(opt_server_tcp_stream.unwrap().unwrap(), max_frame_length);
    let (client_sender, _client_receiver) = tcp_stream_to_conn_pair_01(opt_client_tcp_stream.unwrap(), max_frame_length);

    // Close the client sender:
    drop(client_sender);

    // Expect the server to notice that the connection was closed:
    let mut server_receiver = server_receiver.compat();
    let opt_message = await!(server_receiver.next());
    assert!(opt_message.is_none());
}

#[test]
fn test_tcp_stream_to_conn_pair_drop_sender() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_tcp_stream_to_conn_pair_drop_sender(thread_pool.clone()));
}


