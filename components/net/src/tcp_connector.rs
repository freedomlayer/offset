use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};

use bytes::Bytes;

use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::funder::messages::TcpAddress;

use futures::compat::{Compat, Compat01As03, Future01CompatExt, Stream01CompatExt};
use futures::{FutureExt, Stream, StreamExt, Sink, SinkExt};
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
fn check_sink_01(sink01: &impl Sink01) {}
fn check_stream_01(stream01: &impl Stream01) {}
fn check_sink_03(sink03: &impl Sink) {}
fn check_stream_03(stream03: &impl Stream) {}
*/

// Utils for checking types:
fn check_sender_01(sender01: &impl Sink01<SinkItem=Vec<u8>, SinkError=()>) {}
fn check_receiver_01(receiver01: &impl Stream01<Item=Vec<u8>, Error=()>) {}

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

            let (mut user_sender_03, mut from_user_sender_03) = mpsc::channel::<Result<Vec<u8>,()>>(0);
            let (mut to_user_receiver_03, mut user_receiver_03) = mpsc::channel::<Result<Vec<u8>,()>>(0);

            // Forward messages from user_sender:
            let from_user_sender_01 = Compat::new(from_user_sender_03)
                .map_err(|_| ());

            let sender_01 = sender_01
                .sink_map_err(|_| ())
                .with(|vec: Vec<u8>| Ok(Bytes::from(vec)));

            check_sender_01(&sender_01);
            check_receiver_01(&from_user_sender_01);

            let send_forward_03 = sender_01
                .send_all(from_user_sender_01)
                .compat()
                .map(|_| ());
            
            let _ = self.spawner.spawn(send_forward_03);


            // Forward messages to user_receiver:
            let to_user_receiver_01 = Compat::new(to_user_receiver_03)
                .sink_map_err(|_| ())
                .with(|vec: Vec<u8>| Ok(Ok(vec)));

            let receiver_01 = receiver_01
                .map_err(|_| ())
                .map(|bytes_mut| bytes_mut.to_vec());

            check_sender_01(&to_user_receiver_01);
            check_receiver_01(&receiver_01);

            let recv_forward_01 = to_user_receiver_01
                .send_all(receiver_01)
                .compat()
                .map(|_| ());


            // We want to give the user sender and receiver of Vec<u8> (And not Result<Vec<u8>,()>),
            // so another adapting layer is required:

            let (user_sender, mut from_user_sender) = mpsc::channel::<Vec<u8>>(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel::<Vec<u8>>(0);

            // Forward user_sender:
            let _ = self.spawner.spawn(async move {
                while let Some(data) = await!(from_user_sender.next()) {
                    if let Err(_) = await!(user_sender_03.send(Ok(data))) {
                        return;
                    }
                }
            });

            // Forward user_receiver:
            let _ = self.spawner.spawn(async move {
                while let Some(Ok(data)) = await!(user_receiver_03.next()) {
                    if let Err(_) = await!(to_user_receiver.send(data)) {
                        return;
                    }
                }
            });

            Some((user_sender, user_receiver))
        })
    }
}

