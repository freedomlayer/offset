use derive_more::From;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;

use base64::{self, URL_SAFE_NO_PAD};

use crate::crypto::PublicKey;

#[derive(Debug)]
pub struct SerStringError;

/// Define conversion to/from String:
macro_rules! str_convert_funcs {
    ($to_string_func:ident, $from_string_func:ident, $conv_type:ident, $conv_len:expr) => {
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
    PublicKey::len()
);

#[derive(Debug, From)]
pub enum StringSerdeError {
    // IoError(io::Error),
    TomlDeError(toml::de::Error),
    TomlSeError(toml::ser::Error),
}

pub fn deserialize_from_string<T>(input: &str) -> Result<T, StringSerdeError>
where
    T: DeserializeOwned,
{
    Ok(toml::from_str(&input)?)
}

pub fn serialize_to_string<T>(t: &T) -> Result<String, StringSerdeError>
where
    T: Serialize,
{
    Ok(toml::to_string(t)?)
}
