use std::convert::TryFrom;

use base64::{self, URL_SAFE_NO_PAD};

use crypto::hash::{HashResult, HASH_RESULT_LEN};
use crypto::hash_lock::{HashedLock, PlainLock, HASHED_LOCK_LEN, PLAIN_LOCK_LEN};
use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};
use crypto::invoice_id::{InvoiceId, INVOICE_ID_LEN};
use crypto::payment_id::{PaymentId, PAYMENT_ID_LEN};
use crypto::rand::{RandValue, RAND_VALUE_LEN};

// TODO: Possibly remove this module into offst-crypto
// Will require the extra base64 dependency in offst-crypto.

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
pub fn from_base64<'t, 'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: TryFrom<Vec<u8>>,
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    let vec = base64::decode_config(&string, URL_SAFE_NO_PAD)
        .map_err(|err| Error::custom(err.to_string()))?;
    T::try_from(vec).map_err(|_| Error::custom("Length mismatch"))
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

str_convert_funcs!(
    signature_to_string,
    string_to_signature,
    Signature,
    SIGNATURE_LEN
);

str_convert_funcs!(
    hash_result_to_string,
    string_to_hash_result,
    HashResult,
    HASH_RESULT_LEN
);

str_convert_funcs!(
    invoice_id_to_string,
    string_to_invoice_id,
    InvoiceId,
    INVOICE_ID_LEN
);

str_convert_funcs!(
    payment_id_to_string,
    string_to_payment_id,
    PaymentId,
    PAYMENT_ID_LEN
);

str_convert_funcs!(
    rand_value_to_string,
    string_to_rand_value,
    RandValue,
    RAND_VALUE_LEN
);

str_convert_funcs!(
    plain_lock_to_string,
    string_to_plain_lock,
    PlainLock,
    PLAIN_LOCK_LEN
);

str_convert_funcs!(
    hashed_lock_to_string,
    string_to_hashed_lock,
    HashedLock,
    HASHED_LOCK_LEN
);

// TODO: How to make the macro work nicely with the private key conversion code?

// TODO: Find a better way to represent private key.
// We currently use [u8; 85] directly because of ring limitations.

/// Convert a private key into a string
pub fn private_key_to_string(private_key: &[u8; 85]) -> String {
    // We have to do this because [u8; 85] doesn't implement AsRef, due to compiler limitations
    let private_key_slice = &private_key[0..85];
    base64::encode_config(&private_key_slice, URL_SAFE_NO_PAD)
}

// TODO: Fix all 85 hacks here

/// Convert a string into a private key
pub fn string_to_private_key(private_key_str: &str) -> Result<[u8; 85], SerStringError> {
    // Decode public key:
    let private_key_vec =
        base64::decode_config(private_key_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if private_key_vec.len() != 85 {
        return Err(SerStringError);
    }
    let mut private_key_array = [0u8; 85];
    private_key_array.copy_from_slice(&private_key_vec[0..85]);
    Ok(private_key_array)
}
