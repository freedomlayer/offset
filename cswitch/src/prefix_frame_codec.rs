extern crate tokio_io;
extern crate bytes;
extern crate byteorder;


use std::io;
use std::mem;
use std::cmp;
use self::bytes::{BytesMut};
use self::tokio_io::codec::{Encoder, Decoder};
use self::byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};


const MAX_FRAME_LEN: usize = 2_u32.pow(20) as usize; // 1 MB

/// Break a stream of bytes into chunks, using prefix length frames.
/// Every frame begins with a 32 bit length prefix, after which the data follows.
/// A magic 32 bit value is the beginning of every frame. (TODO: Is the magic a good idea?)
enum PrefixFrameCodecState {
    Empty,
    CollectingLength {
        // Accumulated length bytes:
        accum_length: Vec<u8>,
    },
    CollectingFrame {
        length: usize,
        // Accumulated frame bytes:
        accum_frame: Vec<u8>,
    }
}

struct PrefixFrameCodec {
    state: PrefixFrameCodecState,
}

enum PrefixFrameCodecError {
    SerializeLengthError(io::Error),
    DeserializeLengthError(io::Error),
}


impl Decoder for PrefixFrameCodec {
    type Item = Vec<u8>;
    type Error = PrefixFrameCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            // Should we add here a check if buf.len() == 0?
            match mem::replace(&mut self.state, PrefixFrameCodecState::Empty) {
                PrefixFrameCodecState::Empty => unreachable!(),
                PrefixFrameCodecState::CollectingLength { accum_length } => {
                    // Try to add as many as possible bytes to accum_length:
                    let missing_length_bytes = 4 - accum_length.len();
                    let bytes_to_read = cmp::min(missing_length_bytes, buf.len());
                    accum_length.extend(buf.split_to(bytes_to_read));

                    if accum_length.len() == 4 {
                        // Done reading length.
                        let rdr = io::Cursor::new(accum_length);
                        let frame_length = match rdr.read_u32::<BigEndian>() {
                            Ok(frame_length) => frame_length,
                            Err(e) => return Err(PrefixFrameCodecError::DeserializeLengthError(e)),
                        };

                        // TODO: Check if frame_length is too big.
                        // If so, return an error.
                        assert!(false);
                        
                        self.state = PrefixFrameCodecState::CollectingFrame {
                            length: frame_length as usize,
                            accum_frame: Vec::new(),
                        };
                        // May continue from here to CollectingFrame state in the next iteration of
                        // the loop.
                    } else {
                        self.state = PrefixFrameCodecState::CollectingLength {
                            accum_length,
                        };
                        return Ok(None);
                    }
                },
                PrefixFrameCodecState::CollectingFrame { length, accum_frame } => {
                    let missing_frame_bytes = length - accum_frame.len();
                    let bytes_to_read = cmp::min(missing_frame_bytes, buf.len());
                    accum_frame.extend(buf.split_to(bytes_to_read));

                    if accum_frame.len() == length {
                        // Done reading frame contents
                        self.state = PrefixFrameCodecState::CollectingLength {
                            accum_length: Vec::new(),
                        };

                        // Return a completed message
                        return Ok(Some(accum_frame));

                    } else {
                        self.state = PrefixFrameCodecState::CollectingFrame {
                            length,
                            accum_frame,
                        };
                        return Ok(None);
                    }
                },
            };
        }
    }
}

impl Encoder for PrefixFrameCodec {
    type Item = Vec<u8>;
    type Error = PrefixFrameCodecError;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), Self::Error> {

        // TODO: Make sure that data length is not too large.
        let wtr = vec![];
        match wtr.write_u32::<BigEndian>(data.len() as u32) {
            Ok(()) => {},
            Err(e) => return Err(PrefixFrameCodecError::SerializeLengthError(e)),
        };


        // buf.put(&my_vec[..]);
        // TODO
    }
}


