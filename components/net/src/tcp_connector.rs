use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::funder::messages::TcpAddress;

use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures::{Stream, StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

// We have this import to make the split() call compile.
use futures_01::stream::{Stream as Stream01};

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

impl<S> FutTransform  for TcpConnector<S> 
where
    S: Spawn,
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

            let (receiver, sender) = Framed::new(tcp_stream, codec).split();

            let (user_sender, from_user_sender) = mpsc::channel::<Vec<u8>>(0);
            let (to_user_receiver, user_receiver) = mpsc::channel::<Vec<u8>>(0);

            // Forward messages from user_sender:
            self.spawner.spawn(async move {
                let _ = await!(sender.send_all(&mut from_user_sender));
            });

            // Forward messages to user_receiver:
            self.spawner.spawn(async move {
                let _ = await!(to_user_receiver.send_all(&mut receiver));
            });

            Some((user_sender, user_receiver))
        })

        // TODO: Apply framing on the tcp_stream
    }
}

