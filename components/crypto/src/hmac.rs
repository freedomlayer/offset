//! Hash based message authentication

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

pub fn verify_hmac(data: &[u8], hmac_key: &HmacKey, hmac_result: &HmacResult) -> bool {
    let mut mac = HmacSha512Trunc256::new_varkey(&*hmac_key).expect("Invalid key length");
    mac.update(data);
    mac.verify(&hmac_result).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_basic() {
        let my_data = b"This is my data";

        let my_key1 = HmacKey::from([1u8; 32]);
        let hmac_result1 = create_hmac(&my_data[..], &my_key1);

        let my_key2 = HmacKey::from([2u8; 32]);
        let hmac_result2 = create_hmac(&my_data[..], &my_key2);

        assert!(verify_hmac(my_data, &my_key1, &hmac_result1));
        assert!(verify_hmac(my_data, &my_key2, &hmac_result2));

        assert!(!verify_hmac(my_data, &my_key1, &hmac_result2));
        assert!(!verify_hmac(my_data, &my_key2, &hmac_result1));
    }
}
