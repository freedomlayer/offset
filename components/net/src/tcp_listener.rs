use std::net::SocketAddr;
use bytes::Bytes;
use tokio::net::{TcpListener as TokioTcpListener, 
                 TcpStream};
use tokio::codec::{Framed, LengthDelimitedCodec};

use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use futures_01::stream::{Stream as Stream01};
use futures_01::sink::{Sink as Sink01};

use common::conn::{Listener, ConnPairVec};
use proto::funder::messages::TcpAddress;
use crate::types::tcp_address_to_socket_addr;

use futures::compat::{Compat, Stream01CompatExt};

use crate::compat_utils::conn_pair_01_to_03;


/// Listen for incoming TCP connections
pub struct TcpListener<S> {
    max_frame_length: usize,
    spawner: S,
}

impl<S> TcpListener<S> {
    pub fn new(max_frame_length: usize,
               spawner: S) -> Self {
            
        TcpListener {
            max_frame_length,
            spawner,
        }
    }
}

impl<S> Listener for TcpListener<S> 
where
    S: Spawn + Send + Clone + 'static,
{
    type Connection = ConnPairVec;
    type Config = ();
    type Arg = TcpAddress;

    fn listen(mut self, tcp_address: Self::Arg) -> (mpsc::Sender<Self::Config>,
                                        mpsc::Receiver<Self::Connection>) {

        let (config_sender, mut config_sender_receiver) = mpsc::channel(0);
        let (mut conn_receiver_sender, conn_receiver) = mpsc::channel(0);

        let socket_addr = tcp_address_to_socket_addr(&tcp_address);
        let listener = match TokioTcpListener::bind(&socket_addr) {
            Ok(listener) => listener,
            Err(_) => {
                // Return empty channels:
                return (config_sender, conn_receiver);
            }
        };

        let mut incoming_conns = listener.incoming().compat();
        let mut c_spawner = self.spawner.clone();
        let c_max_frame_length = self.max_frame_length;
        self.spawner.spawn(async move {
            while let Some(Ok(tcp_stream)) = await!(incoming_conns.next()) {

                let mut codec = LengthDelimitedCodec::new();
                codec.set_max_frame_length(c_max_frame_length);
                let (sender_01, receiver_01) = Framed::new(tcp_stream, codec).split();

                // Conversion layer between Vec<u8> to Bytes:
                let sender_01 = sender_01
                    .sink_map_err(|_| ())
                    .with(|vec: Vec<u8>| -> Result<Bytes, ()> {
                        Ok(Bytes::from(vec))
                    });

                let receiver_01 = receiver_01
                    .map(|bytes| bytes.to_vec());

                let conn_pair = conn_pair_01_to_03((sender_01, receiver_01), &mut c_spawner);
                if let Err(_) = await!(conn_receiver_sender.send(conn_pair)) {
                    return;
                }
            }
        });

        (config_sender, conn_receiver)
    }
}
