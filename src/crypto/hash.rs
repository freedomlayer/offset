use ring::digest::{digest, SHA512_256};

pub const HASH_RESULT_LEN: usize = 32;

define_fixed_bytes!(HashResult, HASH_RESULT_LEN);

/// Calculate SHA512/256 over the given data.
pub fn sha_512_256(data: &[u8]) -> HashResult {
    let mut inner = [0x00; HASH_RESULT_LEN];

    let digest_res = digest(&SHA512_256, data);
    inner.copy_from_slice(digest_res.as_ref());

    HashResult(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_basic() {
        let data = b"This is a test!";

        let hash_res = sha_512_256(&data[..]);

        let expected = [
            0x34, 0x9c, 0x7e, 0xa7, 0x49, 0x8d, 0x04, 0x32, 0xdc, 0xb0, 0x60, 0x4a, 0x9e, 0xd3,
            0x7a, 0x8b, 0x65, 0xa9, 0x0b, 0xfa, 0x16, 0x6c, 0x91, 0x47, 0x5f, 0x07, 0x2a, 0x29,
            0xe4, 0x2d, 0xa1, 0xfb,
        ];

        assert_eq!(hash_res.as_ref(), expected);
    }
}
