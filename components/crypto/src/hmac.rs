use hmac::{Hmac, Mac, NewMac};
use proto::crypto::{HmacKey, HmacResult};
use sha2::Sha512Trunc256;

// Create alias for HMAC-SHA512-256
type HmacSha512Trunc256 = Hmac<Sha512Trunc256>;

pub fn create_hmac(data: &[u8], hmac_key: &HmacKey) -> HmacResult {
    let mut mac = HmacSha512Trunc256::new_varkey(&*hmac_key).expect("Invalid key length");
    mac.update(data);
    let digest_res = mac.finalize().into_bytes();

    let mut inner = [0x00; HmacResult::len()];
    inner.copy_from_slice(digest_res.as_ref());

    HmacResult::from(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_basic() {
        let my_data = b"This is my data";
        let my_key = HmacKey::from([1u8; 32]);
        let _hmac_result = create_hmac(&my_data[..], &my_key);
    }
}
