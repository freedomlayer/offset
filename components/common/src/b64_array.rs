// Based on https://stackoverflow.com/questions/48782047/binary-deserialization-of-u8-128
// Answer by dtoInay

use serde::de::{Deserialize, Deserializer, Error};
use serde::ser::Serializer;

use base64::{self, URL_SAFE_NO_PAD};

/// This code is required to be able to Serialize and Deserialize arrays of size larger than 64.
/// This feature is required for serializing the Signature type, which is of size 64.
/// In the future this might be supported automatically by Rust, or can be done using an external
/// crate.
pub trait B64Array<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

macro_rules! b64_array {
    ($($len:expr,)+) => {
        $(
            impl<'de,> B64Array<'de> for [u8; $len] {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                    where S: Serializer
                {
                    let base64_str = base64::encode_config(&self.as_ref(), URL_SAFE_NO_PAD);
                    serializer.serialize_str(&base64_str)
                }

                fn deserialize<D>(deserializer: D) -> Result<[u8; $len], D::Error>
                    where D: Deserializer<'de>
                {
                    let string = String::deserialize(deserializer)?;
                    let vec = base64::decode_config(&string, URL_SAFE_NO_PAD)
                        .map_err(|err| Error::custom(err.to_string()))?;

                    if vec.len() < $len {
                        Err(Error::custom("Length mismatch"))
                    } else {
                        let mut inner = [0u8; $len];
                        inner.copy_from_slice(&vec[..$len]);
                        Ok(inner)
                    }
                }
            }
        )+
    }
}

b64_array! {
    16,
    32,
    64,
    85,
}
