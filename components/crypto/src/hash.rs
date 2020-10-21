use proto::crypto::HashResult;
use sha2::{Digest, Sha512Trunc256};

pub struct Hasher {
    inner: Sha512Trunc256,
}

impl Hasher {
    pub fn new() -> Self {
        Self {
            inner: Sha512Trunc256::new(),
        }
    }
    pub fn update(&mut self, data: &[u8]) -> &mut Self {
        self.inner.update(data);
        self
    }

    pub fn finalize(&self) -> HashResult {
        let digest_res = self.inner.clone().finalize();

        let mut inner = [0x00; HashResult::len()];
        inner.copy_from_slice(digest_res.as_ref());

        HashResult::from(&inner)
    }
}

// TODO: Possibly remove from interface in the future, use Hasher instead.
// TODO: Possibly choose a more generic name, to allow changes in the future?
/// Calculate SHA512/256 over the given data.
pub fn hash_buffer(data: &[u8]) -> HashResult {
    Hasher::new().update(data).finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_basic() {
        let data = b"This is a test!";

        let hash_res = hash_buffer(&data[..]);

        let expected = [
            0x34, 0x9c, 0x7e, 0xa7, 0x49, 0x8d, 0x04, 0x32, 0xdc, 0xb0, 0x60, 0x4a, 0x9e, 0xd3,
            0x7a, 0x8b, 0x65, 0xa9, 0x0b, 0xfa, 0x16, 0x6c, 0x91, 0x47, 0x5f, 0x07, 0x2a, 0x29,
            0xe4, 0x2d, 0xa1, 0xfb,
        ];

        assert_eq!(hash_res.as_ref(), expected);
    }
}
