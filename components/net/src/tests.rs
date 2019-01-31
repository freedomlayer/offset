use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use futures::task::Spawn;
use futures::executor::ThreadPool;

use crate::tcp_connector::TcpConnector;
use crate::tcp_listener::TcpListener;

use tokio::net::{TcpListener as TokioTcpListener, 
                 TcpStream};


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

async fn task_tcp_client_server_v4<S>(mut spawner: S) 
where
    S: Spawn + Send,
{
    // TODO: 
    // - Start a listener on some available port
    // - Connect to the listener with a new client.
    // - Send and receive messages. Make sure that it works properly
    unimplemented!();
}

#[test]
fn test_tcp_client_server_v4() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_tcp_client_server_v4(thread_pool.clone()));
}

