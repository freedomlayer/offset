use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::net::messages::TcpAddress;

use futures::compat::{Future01CompatExt};
use futures::task::Spawn;

use tokio::net::TcpStream;

use crate::utils::tcp_stream_to_conn_pair;
use crate::types::tcp_address_to_socket_addr;

#[derive(Debug, Clone)]
pub struct TcpConnector<S> {
    max_frame_length: usize,
    spawner: S,
}

impl<S> TcpConnector<S> {
    #[allow(unused)]
    pub fn new(max_frame_length: usize,
           spawner: S) -> Self {

        TcpConnector {
            max_frame_length,
            spawner,
        }
    }
}


impl<S> FutTransform  for TcpConnector<S> 
where
    S: Spawn + Send,
{

    type Input = TcpAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let socket_addr = tcp_address_to_socket_addr(&input);
            let tcp_stream = await!(TcpStream::connect(&socket_addr).compat())
                .ok()?;

            Some(tcp_stream_to_conn_pair(tcp_stream,
                                           self.max_frame_length,
                                           &mut self.spawner))

        })
    }
}

