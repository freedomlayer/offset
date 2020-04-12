use common::conn::{BoxFuture, ConnPairVec, FutTransform};

use futures::task::Spawn;

// use std::net::SocketAddr;
use async_std::net::TcpStream;

use proto::net::messages::NetAddress;

use crate::utils::tcp_stream_to_conn_pair;

#[derive(Debug, Clone)]
pub struct TcpConnector<S> {
    max_frame_length: usize,
    spawner: S,
}

impl<S> TcpConnector<S> {
    pub fn new(max_frame_length: usize, spawner: S) -> Self {
        TcpConnector {
            max_frame_length,
            spawner,
        }
    }
}

impl<S> FutTransform for TcpConnector<S>
where
    S: Spawn + Send,
{
    type Input = NetAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, net_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            info!("TcpConnector: Connecting to {:?}", net_address.as_str());
            let tcp_stream = TcpStream::connect(net_address.as_str()).await.ok()?;

            Some(tcp_stream_to_conn_pair(
                tcp_stream,
                self.max_frame_length,
                &mut self.spawner,
            ))
        })
    }
}
