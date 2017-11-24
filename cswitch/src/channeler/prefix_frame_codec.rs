extern crate tokio_io;
extern crate bytes;
extern crate byteorder;

use std::io;
use std::mem;
use std::cmp;
use self::bytes::{BytesMut, BufMut};
use self::tokio_io::codec::{Encoder, Decoder};
use self::byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};


const MAX_FRAME_LEN: usize = 1 << 20;

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


pub struct PrefixFrameCodec {
    state: PrefixFrameCodecState,
}


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
            state: PrefixFrameCodecState::CollectingLength {
                accum_length: Vec::new(),
            },
        }
    }
}

/// Conversion of io::Error into PrefixFrameCodecError.
/// This is required for usage of PrefixFrameCodecError as the error type of PrefixFrameCodec
/// Encoder and Decoder.
impl From<io::Error> for PrefixFrameCodecError {
    fn from(e: io::Error) -> Self {
        PrefixFrameCodecError::IoError(e)
    }
}

impl Decoder for PrefixFrameCodec {
    type Item = Vec<u8>;
    type Error = PrefixFrameCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            // Should we add here a check if buf.len() == 0?
            match mem::replace(&mut self.state, PrefixFrameCodecState::Empty) {
                PrefixFrameCodecState::Empty => unreachable!(),
                PrefixFrameCodecState::CollectingLength { mut accum_length } => {
                    // Try to add as many as possible bytes to accum_length:
                    let missing_length_bytes = 4 - accum_length.len();
                    let bytes_to_read = cmp::min(missing_length_bytes, buf.len());
                    accum_length.extend(buf.split_to(bytes_to_read));

                    if accum_length.len() == 4 {
                        // Done reading length.
                        let mut rdr = io::Cursor::new(accum_length);
                        let frame_length = match rdr.read_u32::<BigEndian>() {
                            Ok(frame_length) => frame_length,
                            Err(e) => return Err(PrefixFrameCodecError::DeserializeLengthError(e)),
                        } as usize;

                        if frame_length > MAX_FRAME_LEN {
                            return Err(PrefixFrameCodecError::ReceivedFrameLenTooLarge);
                        }
                        
                        self.state = PrefixFrameCodecState::CollectingFrame {
                            length: frame_length,
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
                PrefixFrameCodecState::CollectingFrame { length, mut accum_frame } => {
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
        if data.len() > MAX_FRAME_LEN {
            return Err(PrefixFrameCodecError::SentFrameLenTooLarge);
        }

        // Encode length prefix as bytes:
        let mut wtr = vec![];
        match wtr.write_u32::<BigEndian>(data.len() as u32) {
            Ok(()) => {},
            Err(e) => return Err(PrefixFrameCodecError::SerializeLengthError(e)),
        };

        // Write length prefix:
        buf.extend(&wtr[..]);
        // Write actual data:
        buf.extend(&data[..]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_frame_encoder_basic() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(vec![1,2,3,4,5], &mut buf) {
            Ok(()) => {},
            Err(_) => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,5,1,2,3,4,5]);
    }


    #[test]
    fn test_prefix_frame_encoder_empty_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(vec![], &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf, vec![0,0,0,0]);
    }

    #[test]
    fn test_prefix_frame_encoder_large_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(vec![0; MAX_FRAME_LEN], &mut buf) {
            Ok(()) => {},
            _ => panic!("Error encoding data!"),
        };
        assert_eq!(buf.len(), 4 + MAX_FRAME_LEN);
    }

    #[test]
    fn test_prefix_frame_encoder_too_large_data() {
        let mut prefix_frame_codec = PrefixFrameCodec::new();
        let mut buf = BytesMut::new();
        match prefix_frame_codec.encode(vec![0; MAX_FRAME_LEN + 1], &mut buf) {
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
