use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use futures::executor::{ThreadPool, LocalPool};
use futures::task::Spawn;
use futures::{SinkExt, StreamExt};

use common::conn::{FutTransform, Listener};
use proto::net::messages::NetAddress;

use crate::net_connector::NetConnector;
use crate::tcp_connector::TcpConnector;
use crate::tcp_listener::TcpListener;

use async_std::net::TcpListener as AsyncStdTcpListener;

/// Get an available port we can listen on
async fn get_available_port_v4() -> u16 {
    // Idea based on code at:
    // https://github.com/rust-lang-nursery/rust-cookbook/pull/137/files
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    // Assigning port 0 requests the OS to assign a free port
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 0);
    let listener = AsyncStdTcpListener::bind(&socket_addr).await.unwrap();
    listener.local_addr().unwrap().port()
}

const TEST_MAX_FRAME_LEN: usize = 0x100;

async fn task_tcp_client_server_v4<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let available_port = get_available_port_v4().await;
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut tcp_connector = TcpConnector::new(TEST_MAX_FRAME_LEN, spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    for _ in 0..5 {
        let (mut client_sender, mut client_receiver) =
            tcp_connector.transform(socket_addr.clone()).await.unwrap();
        let (mut server_sender, mut server_receiver) = incoming_connections.next().await.unwrap();

        client_sender.send(vec![1, 2, 3]).await.unwrap();
        assert_eq!(server_receiver.next().await.unwrap(), vec![1, 2, 3]);

        server_sender.send(vec![3, 2, 1]).await.unwrap();
        assert_eq!(client_receiver.next().await.unwrap(), vec![3, 2, 1]);
    }

    /*
    // Dropping incoming_connections should close the listener after a while
    drop(incoming_connections);

    // TODO: Do we want the tcp_listener to be closed immediately when incoming_connections is
    // dropped? Is this possible?
    for _ in 0 .. 5 {
        tcp_connector.transform(socket_addr.clone()).await;
    }
    assert!(tcp_connector.transform(socket_addr.clone()).await.is_none());
    */
}

#[test]
fn test_tcp_client_server_v4() {
    let thread_pool = ThreadPool::new().unwrap();
    LocalPool::new().run_until(task_tcp_client_server_v4(thread_pool.clone()));
}

async fn task_net_connector_v4_basic<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let available_port = get_available_port_v4().await;
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut net_connector = NetConnector::new(TEST_MAX_FRAME_LEN, spawner.clone(), spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    let net_address: NetAddress = format!("127.0.0.1:{}", available_port).try_into().unwrap();

    for _ in 0..5 {
        let (mut client_sender, mut client_receiver) =
            net_connector.transform(net_address.clone()).await.unwrap();
        let (mut server_sender, mut server_receiver) = incoming_connections.next().await.unwrap();

        client_sender.send(vec![1, 2, 3]).await.unwrap();
        assert_eq!(server_receiver.next().await.unwrap(), vec![1, 2, 3]);

        server_sender.send(vec![3, 2, 1]).await.unwrap();
        assert_eq!(client_receiver.next().await.unwrap(), vec![3, 2, 1]);
    }
}

#[test]
fn test_net_connector_v4_basic() {
    let thread_pool = ThreadPool::new().unwrap();
    LocalPool::new().run_until(task_net_connector_v4_basic(thread_pool.clone()));
}

async fn task_net_connector_v4_drop_sender<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let available_port = get_available_port_v4().await;
    let loopback = Ipv4Addr::new(127, 0, 0, 1);
    let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);

    let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
    let mut net_connector = NetConnector::new(TEST_MAX_FRAME_LEN, spawner.clone(), spawner.clone());

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    let net_address: NetAddress = format!("127.0.0.1:{}", available_port).try_into().unwrap();

    let (client_sender, _client_receiver) =
        net_connector.transform(net_address.clone()).await.unwrap();
    let (_server_sender, mut server_receiver) = incoming_connections.next().await.unwrap();

    // Drop the client's sender:
    drop(client_sender);

    // Wait until the server understands the connection is closed.
    // This should happen quickly.
    while let Some(_) = server_receiver.next().await {}
}

#[test]
fn test_net_connector_v4_drop_sender() {
    // env_logger::init();
    let thread_pool = ThreadPool::new().unwrap();
    LocalPool::new().run_until(task_net_connector_v4_drop_sender(thread_pool.clone()));
}
