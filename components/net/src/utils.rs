// TODO: Remove this later
#![allow(unused)]
// use bytes::Bytes;

// use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
// use futures::{FutureExt, SinkExt, StreamExt};

// use tokio::codec::{Framed, LengthDelimitedCodec};
// use tokio::net::TcpStream;

use async_std::net::TcpStream;

use common::conn::ConnPairVec;

pub fn tcp_stream_to_conn_pair<S>(
    tcp_stream: TcpStream,
    max_frame_length: usize,
    spawner: &mut S,
) -> ConnPairVec
where
    S: Spawn + Send,
{
    // TODO
    unimplemented!();
    /*
    let mut codec = LengthDelimitedCodec::new();
    codec.set_max_frame_length(max_frame_length);
    let (sender_01, receiver_01) = Framed::new(tcp_stream, codec).split();

    // Conversion layer between Vec<u8> to Bytes:
    let sender_01 = sender_01
        .sink_map_err(|_| ())
        .with(|vec: Vec<u8>| -> Result<Bytes, ()> { Ok(Bytes::from(vec)) });

    let receiver_01 = receiver_01.map(|bytes| bytes.to_vec());

    (sender_01, receiver_01)
    */
}
