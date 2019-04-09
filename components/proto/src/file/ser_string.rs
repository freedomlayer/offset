use base64::{self, URL_SAFE_NO_PAD};
use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crypto::hash::{HashResult, HASH_RESULT_LEN};
use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};
use crypto::invoice_id::{InvoiceId, INVOICE_ID_LEN};

#[derive(Debug)]
pub struct SerStringError;

/// Convert a public key into a string
pub fn public_key_to_string(public_key: &PublicKey) -> String {
    base64::encode_config(&public_key, URL_SAFE_NO_PAD)
}

/// Convert a string into a public key
pub fn string_to_public_key(pk_str: &str) -> Result<PublicKey, SerStringError> {
    // Decode public key:
    let public_key_vec =
        base64::decode_config(pk_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if public_key_vec.len() != PUBLIC_KEY_LEN {
        return Err(SerStringError);
    }
    let mut public_key_array = [0u8; PUBLIC_KEY_LEN];
    public_key_array.copy_from_slice(&public_key_vec[0..PUBLIC_KEY_LEN]);
    Ok(PublicKey::from(&public_key_array))
}

/// Convert a Signature into a string
pub fn signature_to_string(signature: &Signature) -> String {
    base64::encode_config(&signature, URL_SAFE_NO_PAD)
}

/// Convert a string into a signature
pub fn string_to_signature(signature_str: &str) -> Result<Signature, SerStringError> {
    // Decode public key:
    let signature_vec =
        base64::decode_config(signature_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if signature_vec.len() != SIGNATURE_LEN {
        return Err(SerStringError);
    }
    let mut signature_array = [0u8; SIGNATURE_LEN];
    signature_array.copy_from_slice(&signature_vec[0..SIGNATURE_LEN]);
    Ok(Signature::from(&signature_array))
}

/// Convert a HashResult into a string
pub fn hash_result_to_string(hash_result: &HashResult) -> String {
    base64::encode_config(&hash_result, URL_SAFE_NO_PAD)
}

/// Convert a string into a HashResult
pub fn string_to_hash_result(hash_result_str: &str) -> Result<HashResult, SerStringError> {
    // Decode public key:
    let hash_result_vec =
        base64::decode_config(hash_result_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if hash_result_vec.len() != HASH_RESULT_LEN {
        return Err(SerStringError);
    }
    let mut hash_result_array = [0u8; HASH_RESULT_LEN];
    hash_result_array.copy_from_slice(&hash_result_vec[0..HASH_RESULT_LEN]);
    Ok(HashResult::from(&hash_result_array))
}

/// Convert a InvoiceId into a string
pub fn invoice_id_to_string(invoice_id: &InvoiceId) -> String {
    base64::encode_config(&invoice_id, URL_SAFE_NO_PAD)
}

/// Convert a string into a InvoiceId
pub fn string_to_invoice_id(invoice_id_str: &str) -> Result<InvoiceId, SerStringError> {
    // Decode public key:
    let invoice_id_vec =
        base64::decode_config(invoice_id_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if invoice_id_vec.len() != INVOICE_ID_LEN {
        return Err(SerStringError);
    }
    let mut invoice_id_array = [0u8; INVOICE_ID_LEN];
    invoice_id_array.copy_from_slice(&invoice_id_vec[0..INVOICE_ID_LEN]);
    Ok(InvoiceId::from(&invoice_id_array))
}

/// Convert a RandValue into a string
pub fn rand_value_to_string(rand_value: &RandValue) -> String {
    base64::encode_config(&rand_value, URL_SAFE_NO_PAD)
}

/// Convert a string into a RandValue
pub fn string_to_rand_value(rand_value_str: &str) -> Result<RandValue, SerStringError> {
    // Decode public key:
    let rand_value_vec =
        base64::decode_config(rand_value_str, URL_SAFE_NO_PAD).map_err(|_| SerStringError)?;
    // TODO: A more idiomatic way to do this?
    if rand_value_vec.len() != RAND_VALUE_LEN {
        return Err(SerStringError);
    }
    let mut rand_value_array = [0u8; RAND_VALUE_LEN];
    rand_value_array.copy_from_slice(&rand_value_vec[0..RAND_VALUE_LEN]);
    Ok(RandValue::from(&rand_value_array))
}
