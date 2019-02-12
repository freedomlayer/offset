use std::convert::TryInto;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use futures::task::Spawn;
use futures::executor::ThreadPool;
use futures::{StreamExt, SinkExt};

use common::conn::{Listener, FutTransform};
use proto::net::messages::NetAddress;

use crate::tcp_connector::TcpConnector;
use crate::tcp_listener::TcpListener;
use crate::net_connector::NetConnector;

use tokio::net::{TcpListener as TokioTcpListener};


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

    for _ in 0 .. 5 {
        let (mut client_sender, mut client_receiver) = await!(tcp_connector.transform(socket_addr.clone())).unwrap();
        let (mut server_sender, mut server_receiver) = await!(incoming_connections.next()).unwrap();

        await!(client_sender.send(vec![1,2,3])).unwrap();
        assert_eq!(await!(server_receiver.next()).unwrap(), vec![1,2,3]);

        await!(server_sender.send(vec![3,2,1])).unwrap();
        assert_eq!(await!(client_receiver.next()).unwrap(), vec![3,2,1]);
    }

    // Dropping incoming_connections should close the listener after a while
    drop(incoming_connections);

    // TODO: Do we want the tcp_listener to be closed immediately when incoming_connections is
    // dropped? Is this possible?
    for _ in 0 .. 5 {
        await!(tcp_connector.transform(socket_addr.clone()));
    }
    assert!(await!(tcp_connector.transform(socket_addr.clone())).is_none());
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
    let mut net_connector = NetConnector::new(TEST_MAX_FRAME_LEN, spawner.clone()).unwrap();

    let (_config_sender, mut incoming_connections) = tcp_listener.listen(socket_addr.clone());

    let net_address: NetAddress = format!("localhost:{}", available_port).try_into().unwrap();

    for _ in 0 .. 5 {
        let (mut client_sender, mut client_receiver) = await!(net_connector.transform(net_address.clone())).unwrap();
        let (mut server_sender, mut server_receiver) = await!(incoming_connections.next()).unwrap();

        await!(client_sender.send(vec![1,2,3])).unwrap();
        assert_eq!(await!(server_receiver.next()).unwrap(), vec![1,2,3]);

        await!(server_sender.send(vec![3,2,1])).unwrap();
        assert_eq!(await!(client_receiver.next()).unwrap(), vec![3,2,1]);
    }
}

#[test]
fn test_net_connector_v4_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_net_connector_v4_basic(thread_pool.clone()));
}
