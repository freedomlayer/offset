use std::convert::TryFrom;
use std::str::FromStr;
use std::string::ToString;

use base64::{self, URL_SAFE_NO_PAD};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

#[derive(Debug)]
pub struct SerStringError;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serializer};

/// Serializes `buffer` to a lowercase hex string.
pub fn to_base64<T, S>(to_base64: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: Serializer,
{
    let base64_str = base64::encode_config(&to_base64.as_ref(), URL_SAFE_NO_PAD);
    serializer.serialize_str(&base64_str)
}

/// Deserializes a lowercase hex string to a `Vec<u8>`.
pub fn from_base64<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: for<'t> TryFrom<&'t [u8]>,
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    let vec = base64::decode_config(&string, URL_SAFE_NO_PAD)
        .map_err(|err| Error::custom(err.to_string()))?;
    T::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))
}

/// Serializes value as a string
pub fn to_string<T, S>(input: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer,
{
    serializer.serialize_str(&input.to_string())
}

/// Deserializes a string into a value
pub fn from_string<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(|_| Error::custom("Failed to parse as string"))
}

/// Define conversion to/from String:
macro_rules! str_convert_funcs {
    ($to_string_func:ident, $from_string_func:ident, $conv_type:ident, $conv_len:ident) => {
        /// Convert a our type into a string
        pub fn $to_string_func(conv: &$conv_type) -> String {
            base64::encode_config(&conv, URL_SAFE_NO_PAD)
        }

        /// Convert a string into a our type
        pub fn $from_string_func(input_str: &str) -> Result<$conv_type, SerStringError> {
            // Decode public key:
            let conv_vec =
                base64::decode_config(input_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
            // TODO: A more idiomatic way to do this?
            if conv_vec.len() != $conv_len {
                return Err(SerStringError);
            }
            let mut conv_array = [0u8; $conv_len];
            conv_array.copy_from_slice(&conv_vec[0..$conv_len]);
            Ok($conv_type::from(&conv_array))
        }
    };
}

str_convert_funcs!(
    public_key_to_string,
    string_to_public_key,
    PublicKey,
    PUBLIC_KEY_LEN
);
