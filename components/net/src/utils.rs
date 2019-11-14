// TODO: Remove this later
#![allow(unused)]
use bytes::Bytes;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, FutureExt, SinkExt, StreamExt, TryFutureExt, TryStreamExt};
use futures_codec::{Framed, LengthCodec};

// use tokio::codec::{Framed, LengthDelimitedCodec};
// use tokio::net::TcpStream;

use async_std::net::TcpStream;

use common::conn::ConnPairVec;

pub fn tcp_stream_to_conn_pair<S>(
    tcp_stream: TcpStream,
    _max_frame_length: usize,
    spawner: &mut S,
) -> ConnPairVec
where
    S: Spawn + Send,
{
    // TODO: Return support for max_frame_length
    let mut codec = LengthCodec;
    // codec.set_max_frame_length(max_frame_length);
    let (sender, receiver) = Framed::new(tcp_stream, codec).split();

    // Conversion layer between Vec<u8> to Bytes:
    let mut vec_sender =
        sender
            .sink_map_err(|_| ())
            .with(|vec: Vec<u8>| -> future::Ready<Result<Bytes, ()>> {
                future::ready(Ok(Bytes::from(vec)))
            });

    let mut vec_receiver = receiver.map_ok(|bytes| bytes.to_vec())
                                .map_err(|_| ());

    let (user_sender, local_receiver) = mpsc::channel::<Vec<u8>>(1);
    let (local_sender, user_receiver) = mpsc::channel::<Vec<u8>>(1);


    spawner.spawn(async move {
        local_sender
            .sink_map_err(|_| ())
            .send_all(&mut vec_receiver)
            .map_err(|e| warn!("Failure to send to local_sender: {:?}", e))
            .await;
    });

    spawner.spawn(async move {
        vec_sender
            .send_all(&mut local_receiver.map(Ok))
            .map_err(|e| warn!("Failure to send to vec_sender: {:?}", e))
            .await;
    });

    (user_sender, user_receiver)
}
