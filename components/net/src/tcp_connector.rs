use bytes::Bytes;

use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::funder::messages::TcpAddress;

use futures::compat::{Future01CompatExt};
use futures::task::Spawn;

use futures_01::stream::{Stream as Stream01};
use futures_01::sink::{Sink as Sink01};


use tokio::net::TcpStream;
use tokio::codec::{Framed, LengthDelimitedCodec};

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


/*
fn check_sink_01(sink01: &impl Sink01) {}
fn check_stream_01(stream01: &impl Stream01) {}
fn check_sink_03(sink03: &impl Sink) {}
fn check_stream_03(stream03: &impl Stream) {}
*/

// Utils for checking types:
// fn check_sender_01(sender01: &impl Sink01<SinkItem=Vec<u8>, SinkError=()>) {}
// fn check_receiver_01(receiver01: &impl Stream01<Item=Vec<u8>, Error=()>) {}


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

