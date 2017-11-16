extern crate tokio_io;
extern crate bytes;

use std::io;
use self::bytes::{BytesMut};
use self::tokio_io::codec::{Encoder, Decoder};

// TODO: Add a maximum message length?

/// Break a stream of bytes into chunks, using prefix length frames.
/// Every frame begins with a 32 bit length prefix, after which the data follows.
/// A magic 32 bit value is the beginning of every frame. (TODO: Is the magic a good idea?)
enum PrefixFrameCodec {
    CollectingLength {
        // Accumulated length bytes:
        accum_length: Vec<u8>,
    },
    CollectingFrame {
        length: u32,
        // Accumulated frame bytes:
        accum_frame: Vec<u8>,
    }
}


impl Decoder for PrefixFrameCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        // TODO
    }
}

impl Encoder for PrefixFrameCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> io::Result<()> {
        // TODO
    }
}


