use bytes::Bytes;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, SinkExt, StreamExt};
use futures_codec::{Framed, LengthCodec};

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
    let codec = LengthCodec;
    // codec.set_max_frame_length(max_frame_length);
    let (sender, receiver) = Framed::new(tcp_stream, codec).split();

    // Conversion layer between Vec<u8> to Bytes:
    let mut vec_sender =
        sender
            .sink_map_err(|_| ())
            .with(|vec: Vec<u8>| -> future::Ready<Result<Bytes, ()>> {
                future::ready(Ok(Bytes::from(vec)))
            });

    let vec_receiver = receiver
        .take_while(|res| future::ready(res.is_ok()))
        .map(|res| res.unwrap().to_vec());

    // Add a mechanism to make sure that the connection is dropped when we drop the sender.
    // Not fully sure why this doesn't happen automatically.
    let (user_sender, user_sender_receiver) = mpsc::channel(0);
    let (mut user_receiver_sender, user_receiver) = mpsc::channel(0);

    let receiver_task = spawner
        .spawn_with_handle(async move {
            let _ = user_receiver_sender
                .send_all(&mut vec_receiver.map(Ok))
                .await;
        })
        .unwrap();

    spawner
        .spawn(async move {
            let _ = vec_sender.send_all(&mut user_sender_receiver.map(Ok)).await;
            drop(receiver_task);
        })
        .unwrap();

    ConnPairVec::from_raw(user_sender, user_receiver)
}
