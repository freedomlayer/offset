use base64::{self, URL_SAFE_NO_PAD};
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

#[derive(Debug)]
pub struct PkStringError;

/// Convert a public key into a string
pub fn public_key_to_string(public_key: &PublicKey) -> String {
    base64::encode_config(&public_key, URL_SAFE_NO_PAD)
}

/// Convert a string into a public key
pub fn string_to_public_key(pk_str: &str) -> Result<PublicKey, PkStringError> {
    // Decode public key:
    let public_key_vec = base64::decode_config(pk_str, URL_SAFE_NO_PAD)
        .map_err(|_| PkStringError)?;
    // TODO: A more idiomatic way to do this?
    if public_key_vec.len() != PUBLIC_KEY_LEN {
        return Err(PkStringError);
    }
    let mut public_key_array = [0u8; PUBLIC_KEY_LEN];
    public_key_array.copy_from_slice(&public_key_vec[0 .. PUBLIC_KEY_LEN]);
    Ok(PublicKey::from(&public_key_array))

}
