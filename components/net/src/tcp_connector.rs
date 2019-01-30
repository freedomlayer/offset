use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};

use bytes::Bytes;

use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::funder::messages::TcpAddress;

use futures::compat::{Compat, Compat01As03, Future01CompatExt, Stream01CompatExt};
use futures::{Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

// We have this import to make the split() call compile.
use futures_01::stream::{Stream as Stream01};
use futures_01::sink::{Sink as Sink01};

use tokio::net::TcpStream;
use tokio::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone)]
pub struct TcpConnector<S> {
    max_frame_length: usize,
    spawner: S,
}

impl<S> TcpConnector<S> {
    fn new(max_frame_length: usize,
           spawner: S) -> Self {

        TcpConnector {
            max_frame_length,
            spawner,
        }
    }
}

/// Convert offst's TcpAddress to SocketAddr
fn tcp_address_to_socket_addr(tcp_address: &TcpAddress) -> SocketAddr {
    match tcp_address {
        TcpAddress::V4(tcp_address_v4) => {
            let address = &tcp_address_v4.address;
            let ipv4_addr = Ipv4Addr::new(address[0], address[1], address[2], address[3]);
            SocketAddr::new(IpAddr::V4(ipv4_addr), tcp_address_v4.port)
        },
        TcpAddress::V6(tcp_address_v6) => {
            let address = &tcp_address_v6.address;
            // TODO: A more elegant way to write this? :
            let ipv4_addr = Ipv6Addr::new(((address[ 0] as u16) << 8) + (address[ 1] as u16), 
                                          ((address[ 2] as u16) << 8) + (address[ 3] as u16),
                                          ((address[ 4] as u16) << 8) + (address[ 5] as u16),
                                          ((address[ 6] as u16) << 8) + (address[ 7] as u16),
                                          ((address[ 8] as u16) << 8) + (address[ 9] as u16),
                                          ((address[10] as u16) << 8) + (address[11] as u16),
                                          ((address[12] as u16) << 8) + (address[13] as u16),
                                          ((address[14] as u16) << 8) + (address[15] as u16));
            SocketAddr::new(IpAddr::V6(ipv4_addr), tcp_address_v6.port)
        },
    }
}


/*
fn stream_01_to_03(stream: impl Stream01) -> impl Stream {
    stream.compat()
}

fn sink_01_to_03(sink: impl Sink01) -> impl Sink {
    sink.compat()
}
*/

fn check_sink_01(sink01: &impl Sink01) {}
fn check_stream_01(stream01: &impl Stream01) {}
fn check_sink_03(sink03: &impl Sink) {}
fn check_stream_03(stream03: &impl Stream) {}

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

            let mut codec = LengthDelimitedCodec::new();
            codec.set_max_frame_length(self.max_frame_length);

            let (sender_01, receiver_01) = Framed::new(tcp_stream, codec).split();
            check_sink_01(&sender_01);
            check_stream_01(&receiver_01);

            let (user_sender, from_user_sender) = mpsc::channel::<Result<Vec<u8>,()>>(0);
            let (to_user_receiver, user_receiver) = mpsc::channel::<Result<Vec<u8>,()>>(0);

            // Forward messages from user_sender:
            let from_user_sender_01 = Compat::new(from_user_sender);
            let sender_01 = sender_01
                .with(|vec| Ok(Bytes::from(vec)))
                .sink_map_err(|_| ());

            let send_forward = sender_01.send_all(from_user_sender_01);
            unimplemented!();
            /*
            self.spawner.spawn(send_forward);

            // Forward messages to user_receiver:
            let recv_forward = to_user_receiver.send_all(receiver.compat())
                .map(|_| ());
            self.spawner.spawn(recv_forward);

            Some((user_sender, user_receiver))
            */
        })

        // TODO: Apply framing on the tcp_stream
    }
}

