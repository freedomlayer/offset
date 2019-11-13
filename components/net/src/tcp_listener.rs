use std::net::SocketAddr;

use async_std::net::TcpListener as AsyncStdTcpListener;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use crate::utils::tcp_stream_to_conn_pair;
use common::conn::{ConnPairVec, Listener};


/// Listen for incoming TCP connections
pub struct TcpListener<S> {
    max_frame_length: usize,
    spawner: S,
}

impl<S> TcpListener<S> {
    pub fn new(max_frame_length: usize, spawner: S) -> Self {
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
    type Arg = SocketAddr;

    fn listen(
        self,
        socket_addr: Self::Arg,
    ) -> (mpsc::Sender<Self::Config>, mpsc::Receiver<Self::Connection>) {
        let (config_sender, _config_sender_receiver) = mpsc::channel(0);
        let (mut conn_receiver_sender, conn_receiver) = mpsc::channel(0);


        let mut c_spawner = self.spawner.clone();
        let c_max_frame_length = self.max_frame_length;
        let _ = self.spawner.spawn(async move {

            let listener = match AsyncStdTcpListener::bind(&socket_addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    warn!("Failed listening on {:?}: {:?}", socket_addr, e);
                    return;
                }
            };
            let mut incoming_conns = listener.incoming();

            while let Some(Ok(tcp_stream)) = incoming_conns.next().await {
                let conn_pair =
                    tcp_stream_to_conn_pair(tcp_stream, c_max_frame_length, &mut c_spawner);
                if let Err(e) = conn_receiver_sender.send(conn_pair).await {
                    warn!("TcpListener::listen(): Send error: {:?}", e);
                    return;
                }
            }
        });

        (config_sender, conn_receiver)
    }
}
