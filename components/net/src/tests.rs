use std::convert::TryFrom;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use futures::channel::mpsc;
use futures::executor::{block_on, ThreadPool};
use futures::task::Spawn;
use futures::{SinkExt, StreamExt};

use common::conn::{ConnPairVec, FutTransform, Listener};
use proto::net::messages::NetAddress;

// use crate::net_connector::NetConnector;
use crate::tcp_connector::TcpConnector;
use crate::tcp_listener::TcpListener;

use async_std::net::TcpListener as AsyncStdTcpListener;
use async_std::task::sleep;

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

async fn get_conn<S>(spawner: S) -> (TcpConnector<S>, mpsc::Receiver<ConnPairVec>, NetAddress)
where
    S: Spawn + Clone + Send + 'static,
{
    // Keep looping until we manage to listen successfuly.
    // This is done to make tests more stable. It seems like sometimes listening will not work,
    // possibly because timing issues with vacant local ports.
    for _ in 0..10usize {
        let available_port = dbg!(get_available_port_v4().await);
        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), available_port);
        let net_address = NetAddress::try_from(format!("127.0.0.1:{}", available_port)).unwrap();

        let tcp_listener = TcpListener::new(TEST_MAX_FRAME_LEN, spawner.clone());
        let mut tcp_connector = TcpConnector::new(TEST_MAX_FRAME_LEN, spawner.clone());

        let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

        dbg!("Try to connect");
        // Try to connect:
        if let Some(_client_conn) = tcp_connector.transform(net_address.clone()).await {
            dbg!("Free connection");
            // Free connection from the other side:
            if let Some(_incoming_conn) = incoming_connections.next().await {
                dbg!("Inside");
                return (tcp_connector, incoming_connections, net_address);
            }
            // If we get here, it probably means that we connected to some other server.
            // In that case we need to close the connection iterate again.
        }
        sleep(Duration::from_millis(100)).await;
    }
    // Give up after a certain amount of attempts:
    unreachable!();
}

async fn task_tcp_client_server_v4<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let (mut tcp_connector, mut incoming_connections, net_address) =
        get_conn(spawner.clone()).await;
    for _ in 0..5usize {
        let (mut client_sender, mut client_receiver) = tcp_connector
            .transform(net_address.clone())
            .await
            .unwrap()
            .split();
        let (mut server_sender, mut server_receiver) =
            incoming_connections.next().await.unwrap().split();

        client_sender.send(vec![1, 2, 3]).await.unwrap();
        assert_eq!(server_receiver.next().await.unwrap(), vec![1, 2, 3]);

        server_sender.send(vec![3, 2, 1]).await.unwrap();
        assert_eq!(client_receiver.next().await.unwrap(), vec![3, 2, 1]);
    }
}

#[test]
fn test_tcp_client_server_v4() {
    env_logger::init();
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_tcp_client_server_v4(thread_pool.clone()));
}

async fn task_net_connector_v4_drop_sender<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let (mut tcp_connector, mut incoming_connections, net_address) =
        get_conn(spawner.clone()).await;

    let (client_sender, _client_receiver) = tcp_connector
        .transform(net_address.clone())
        .await
        .unwrap()
        .split();
    let (_server_sender, mut server_receiver) = incoming_connections.next().await.unwrap().split();

    // Drop the client's sender:
    drop(client_sender);

    // Wait until the server understands the connection is closed.
    // This should happen quickly.
    while let Some(_) = server_receiver.next().await {}
}

#[test]
fn test_net_connector_v4_drop_sender() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_net_connector_v4_drop_sender(thread_pool.clone()));
}
