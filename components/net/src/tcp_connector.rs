use common::conn::{BoxFuture, ConnPairVec, FutTransform};

use futures::task::Spawn;

use std::net::SocketAddr;
use async_std::net::TcpStream;

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
    type Input = SocketAddr;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, socket_addr: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            let tcp_stream = TcpStream::connect(&socket_addr).await.ok()?;

            Some(tcp_stream_to_conn_pair(
                tcp_stream,
                self.max_frame_length,
                &mut self.spawner,
            ))
        })
    }
}
