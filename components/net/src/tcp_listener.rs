use std::net::SocketAddr;

use async_std::net::TcpListener as AsyncStdTcpListener;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use crate::utils::tcp_stream_to_conn_pair;
use common::conn::{BoxFuture, ConnPairVec, ListenClient, Listener};

/// Listen for incoming TCP connections
pub struct TcpListener<S> {
    max_frame_length: usize,
    socket_addr: SocketAddr,
    spawner: S,
}

impl<S> TcpListener<S> {
    pub fn new(max_frame_length: usize, socket_addr: SocketAddr, spawner: S) -> Self {
        TcpListener {
            max_frame_length,
            socket_addr,
            spawner,
        }
    }
}

#[derive(Debug)]
pub struct TcpListenerError;

impl<S> Listener for TcpListener<S>
where
    S: Spawn + Send + Clone + 'static,
{
    type Conn = ConnPairVec;
    type Config = ();
    type Error = TcpListenerError;

    fn listen(
        self,
    ) -> BoxFuture<'static, Result<ListenClient<Self::Config, Self::Conn>, Self::Error>> {
        let (config_sender, _config_sender_receiver) = mpsc::channel(0);
        let (mut conn_receiver_sender, conn_receiver) = mpsc::channel(0);

        Box::pin(async move {
            let mut c_spawner = self.spawner.clone();
            let c_max_frame_length = self.max_frame_length;
            let listener = match AsyncStdTcpListener::bind(&self.socket_addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    warn!("Failed listening on {:?}: {:?}", self.socket_addr, e);
                    return Err(TcpListenerError);
                }
            };

            let _ = self.spawner.spawn(async move {
                let mut incoming_conns = listener.incoming();

                while let Some(Ok(tcp_stream)) = incoming_conns.next().await {
                    info!(
                        "TcpListener: Incoming connection from: {:?}",
                        tcp_stream.peer_addr(),
                    );
                    let conn_pair =
                        tcp_stream_to_conn_pair(tcp_stream, c_max_frame_length, &mut c_spawner);
                    if let Err(e) = conn_receiver_sender.send(conn_pair).await {
                        warn!("TcpListener::listen(): Send error: {:?}", e);
                        return;
                    }
                }
            });

            Ok(ListenClient {
                config_sender,
                conn_receiver,
            })
        })
    }
}
