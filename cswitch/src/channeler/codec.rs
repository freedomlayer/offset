//! The prefix length framed codec.
//!
//! # Frame Format
//!
//! ```ignore
//! +-------------------+-----------------------+
//! |  length(4 bytes)  |  data (length bytes)  |
//! +-------------------+-----------------------+
//! ```
//!
//! - `length` (unsigned 4 bytes integer, big endian): the length (in bytes) of the remaining data
//! - `data` (bytes, with length of `length`): the actually data

use std::{cmp, io, mem};
use tokio_io::codec::{Encoder, Decoder};
use bytes::{Bytes, BytesMut, Buf, BufMut, BigEndian};

const MAX_FRAME_LEN: usize = 1 << 20;

enum State {
    Empty,
    CollectingLength,
    CollectingFrame{
        length: usize,
        frame:  BytesMut,
    }
}

pub struct Codec {
    state: State,
}

#[derive(Debug)]
pub enum CodecError {
    Io(io::Error),
    TooLarge,
}

impl Codec {
    pub fn new() -> Self {
        Codec {
            state: State::CollectingLength,
        }
    }
}

/// Conversion of `io::Error` into `CodecError`.
impl From<io::Error> for CodecError {
    #[inline]
    fn from(e: io::Error) -> Self {
        CodecError::Io(e)
    }
}

impl Decoder for Codec {
    type Item  = Bytes;
    type Error = CodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match mem::replace(&mut self.state, State::Empty) {
                State::Empty => unreachable!(),
                State::CollectingLength => {
                    if buf.len() < 4usize {
                        // Do nothing, left data in buf
                        self.state = State::CollectingLength;
                        return Ok(None);
                    } else {
                        // Consume the first 4 bytes in the buf
                        let length_bytes = buf.split_to(4usize).freeze();
                        let length = io::Cursor::new(length_bytes).get_u32::<BigEndian>() as usize;

                        if length > MAX_FRAME_LEN {
                            return Err(CodecError::TooLarge);
                        }

                        // Transfer state and pre-allocate space needed to collect this frame
                        self.state = State::CollectingFrame {
                            length,
                            frame: BytesMut::with_capacity(length),
                        };
                    }
                }
                State::CollectingFrame { length, mut frame } => {
                    let bytes_to_consume = cmp::min(length - frame.len(), buf.len());

                    frame.put(buf.split_to(bytes_to_consume));

                    if frame.len() == length {
                        self.state = State::CollectingLength;
                        return Ok(Some(frame.freeze()));
                    } else {
                        self.state = State::CollectingFrame { length, frame };
                        return Ok(None);
                    }
                }
            }
        }
    }
}

impl Encoder for Codec {
    type Item  = Bytes;
    type Error = CodecError;

    fn encode(&mut self, data: Bytes, buf: &mut BytesMut) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_LEN {
            return Err(CodecError::TooLarge);
        }

        // Make sure there is enough space in buffer to put data in and
        // avoid more than one allocation caused by using `buf.extend(..)`
        buf.reserve(4 + data.len());

        let mut prefix_length = BytesMut::with_capacity(4usize);
        prefix_length.put_u32::<BigEndian>(data.len() as u32);

        buf.put(prefix_length);
        buf.put(data);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::WriteBytesExt;

    #[test]
    fn test_prefix_frame_encoder_basic() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[1, 2, 3, 4, 5]), &mut buf) {
            Ok(()) => {},
            Err(_) => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,5,1,2,3,4,5]);
    }

    #[test]
    fn test_prefix_frame_encoder_empty_data() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::new(), &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,0]);
    }

    #[test]
    fn test_prefix_frame_encoder_large_data() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[0; MAX_FRAME_LEN]), &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf.len(), 4 + MAX_FRAME_LEN);
    }

    #[test]
    fn test_prefix_frame_encoder_too_large_data() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[0; MAX_FRAME_LEN + 1]), &mut buf) {
            Err(CodecError::SentFrameLenTooLarge) => {},
            _ => panic!("Test failed"),
        };
    }

    #[test]
    fn test_prefix_frame_decoder() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();
        buf.extend(vec![0,0]);
        match prefix_frame_codec.decode(&mut buf) {
            Ok(None) => {},
            _ => panic!("Test failed1!"),
        };
        buf.extend(vec![0]);
        match prefix_frame_codec.decode(&mut buf) {
            Ok(None) => {},
            _ => panic!("Test failed2!"),
        };
        buf.extend(vec![5]);
        match prefix_frame_codec.decode(&mut buf) {
            Ok(None) => {},
            _ => panic!("Test failed3!"),
        };
        buf.extend(vec![1,2,3,4]);
        match prefix_frame_codec.decode(&mut buf) {
            Ok(None) => {},
            _ => panic!("Test failed4!"),
        };
        buf.extend(vec![5,6,7,8]);
        match prefix_frame_codec.decode(&mut buf) {
            Ok(Some(v)) => assert_eq!(v,vec![1,2,3,4,5]),
            _ => panic!("Test failed5!"),
        };

        // Make sure that we still have the remainder:
        assert_eq!(buf, vec![6,7,8]);
    }

    #[test]
    fn test_prefix_frame_decoder_len_too_large() {
        let mut prefix_frame_codec = Codec::new();
        let mut buf = BytesMut::new();

        // Encode length prefix as bytes:
        let mut wtr = vec![];
        wtr.write_u32::<BigEndian>((MAX_FRAME_LEN + 1) as u32).unwrap();
        buf.extend(wtr);
        match prefix_frame_codec.decode(&mut buf) {
            Err(CodecError::TooLarge) => {},
            _ => panic!("Test failed1!"),
        };
    }
}
