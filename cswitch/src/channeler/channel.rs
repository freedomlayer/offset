extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate capnp;

use std::net::SocketAddr;

use self::futures::sync::mpsc;
use self::futures::{Future, IntoFuture, Stream};
use self::tokio_core::net::TcpStream;
use self::tokio_core::reactor::Handle;
use self::tokio_io::AsyncRead;

use ::inner_messages::ChannelerAddress;
use ::crypto::identity::PublicKey;
use ::schema::channeler_capnp::{init_channel};

use super::ToChannel;
use super::prefix_frame_codec::PrefixFrameCodec;

pub enum ChannelError {
}

pub fn create_channel(handle: &Handle, socket_addr: SocketAddr, neighbor_public_key: &PublicKey) 
        -> impl Future<Item=(), Error=ChannelError> {

    // TODO:
    // Create an mpsc channel that will be used to signal this channel future.
    // This line should be added to only after 

    // Attempt a connection:
    TcpStream::connect(&socket_addr, handle)
        .and_then(|stream| {
            let (sink, stream) = stream.framed(PrefixFrameCodec::new()).split();
            // TODO: Create Init Channeler message:
            
            // let mut message = ::capnp::message::Builder::new_default();
            // let init_channel = message.init_root::<init_channel::Builder>();

            
            Ok(())
        });

        // After exchange happened:
        // neighbor.num_pending_out_conn -= 1;
        // let (channel_sender, channel_receiver) = mpsc::channel(0);
        // neighbor.channel_senders.push(AsyncMutex::new(channel_sender));

        // TODO: Binary deserializtion of Channeler to Channeler messages.

    Ok(()).into_future()
}
