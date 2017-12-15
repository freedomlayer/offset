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

enum PrefixFrameCodecState {
    Empty,
    CollectingLength,
    CollectingFrame{
        length: usize,
        frame:  BytesMut,
    }
}

pub struct PrefixFrameCodec {
    state: PrefixFrameCodecState,
}

#[derive(Debug)]
pub enum PrefixFrameCodecError {
    SerializeLengthError(io::Error),
    DeserializeLengthError(io::Error),
    IoError(io::Error),
    ReceivedFrameLenTooLarge,
    SentFrameLenTooLarge,
}

impl PrefixFrameCodec {
    pub fn new() -> Self {
        PrefixFrameCodec {
            state: PrefixFrameCodecState::CollectingLength,
        }
    }
}

/// Conversion of `io::Error` into `PrefixFrameCodecError`.
impl From<io::Error> for PrefixFrameCodecError {
    #[inline]
    fn from(e: io::Error) -> Self {
        PrefixFrameCodecError::IoError(e)
    }
}

impl Decoder for PrefixFrameCodec {
    type Item  = Bytes;
    type Error = PrefixFrameCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match mem::replace(&mut self.state, PrefixFrameCodecState::Empty) {
                PrefixFrameCodecState::Empty => unreachable!(),
                PrefixFrameCodecState::CollectingLength => {
                    if buf.len() < 4usize {
                        // Do nothing, left data in buf
                        self.state = PrefixFrameCodecState::CollectingLength;
                        return Ok(None);
                    } else {
                        // Consume the first 4 bytes in the buf
                        let length_bytes = buf.split_to(4usize).freeze();
                        let length = io::Cursor::new(length_bytes).get_u32::<BigEndian>() as usize;

                        if length > MAX_FRAME_LEN {
                            return Err(PrefixFrameCodecError::ReceivedFrameLenTooLarge);
                        }

                        // Transfer state and pre-allocate space needed to collect this frame
                        self.state = PrefixFrameCodecState::CollectingFrame {
                            length,
                            frame: BytesMut::with_capacity(length),
                        };
                    }
                }
                PrefixFrameCodecState::CollectingFrame { length, mut frame } => {
                    let bytes_to_consume = cmp::min(length - frame.len(), buf.len());

                    frame.put(buf.split_to(bytes_to_consume));

                    if frame.len() == length {
                        self.state = PrefixFrameCodecState::CollectingLength;
                        return Ok(Some(frame.freeze()));
                    } else {
                        self.state = PrefixFrameCodecState::CollectingFrame { length, frame };
                        return Ok(None);
                    }
                }
            }
        }
    }
}

impl Encoder for PrefixFrameCodec {
    type Item  = Bytes;
    type Error = PrefixFrameCodecError;

    fn encode(&mut self, data: Bytes, buf: &mut BytesMut) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_LEN {
            return Err(PrefixFrameCodecError::SentFrameLenTooLarge);
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
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[1, 2, 3, 4, 5]), &mut buf) {
            Ok(()) => {},
            Err(_) => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,5,1,2,3,4,5]);
    }

    #[test]
    fn test_prefix_frame_encoder_empty_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::new(), &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,0]);
    }

    #[test]
    fn test_prefix_frame_encoder_large_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[0; MAX_FRAME_LEN]), &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf.len(), 4 + MAX_FRAME_LEN);
    }

    #[test]
    fn test_prefix_frame_encoder_too_large_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(Bytes::from_static(&[0; MAX_FRAME_LEN + 1]), &mut buf) {
            Err(PrefixFrameCodecError::SentFrameLenTooLarge) => {},
            _ => panic!("Test failed"),
        };
    }

    #[test]
    fn test_prefix_frame_decoder() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
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
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();

        // Encode length prefix as bytes:
        let mut wtr = vec![];
        wtr.write_u32::<BigEndian>((MAX_FRAME_LEN + 1) as u32).unwrap();
        buf.extend(wtr);
        match prefix_frame_codec.decode(&mut buf) {
            Err(PrefixFrameCodecError::ReceivedFrameLenTooLarge) => {},
            _ => panic!("Test failed1!"),
        };
    }
}
