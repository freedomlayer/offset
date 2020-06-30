use std::io;
use std::net::SocketAddr;

use async_std::net::TcpListener as AsyncStdTcpListener;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use crate::utils::tcp_stream_to_conn_pair;
use common::conn::{ConnPairVec, FutListenerClient, Listener, ListenerClient};

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

#[derive(Debug)]
pub enum TcpListenerError {
    BindError(SocketAddr, io::Error),
    SpawnError,
}

impl<S> Listener for TcpListener<S>
where
    S: Spawn + Send + Clone + 'static,
{
    type Connection = ConnPairVec;
    type Config = ();
    type Error = TcpListenerError;
    type Arg = SocketAddr;

    fn listen(
        self,
        socket_addr: Self::Arg,
    ) -> FutListenerClient<Self::Config, Self::Connection, Self::Error> {
        let (config_sender, _config_sender_receiver) = mpsc::channel(0);
        let (mut conn_receiver_sender, conn_receiver) = mpsc::channel(0);

        let mut c_spawner = self.spawner.clone();
        let c_max_frame_length = self.max_frame_length;
        Box::pin(async move {
            let listener = AsyncStdTcpListener::bind(&socket_addr)
                .await
                .map_err(|error| TcpListenerError::BindError(socket_addr, error))?;

            self.spawner
                .spawn(async move {
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
                })
                .map_err(|_| TcpListenerError::SpawnError)?;

            Ok(ListenerClient {
                config_sender,
                conn_receiver,
            })
        })
    }
}
