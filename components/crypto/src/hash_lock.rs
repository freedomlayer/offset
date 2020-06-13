use sha2::{Digest, Sha512Trunc256};

use proto::crypto::{HashedLock, PlainLock};

use crate::hash::sha_512_256;

pub trait HashLock {
    /// Lock the plain hash
    fn hash_lock(&self) -> HashedLock;
}

impl HashLock for PlainLock {
    // TODO: Use bcrypt instead here? (As suggested by @spolu)
    fn hash_lock(&self) -> HashedLock {
        let mut hashed_lock = HashedLock::default();
        let inner = &mut hashed_lock;

        let mut hasher = Sha512Trunc256::new();
        hasher.update(&self);
        let digest_res = hasher.finalize();

        inner.copy_from_slice(digest_res.as_ref());
        hashed_lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proto::crypto::PlainLock;

    #[test]
    fn test_hash_lock_basic() {
        let plain_lock = PlainLock::from(&[1u8; PlainLock::len()]);
        let hashed_lock1 = plain_lock.hash_lock();
        let hashed_lock2 = plain_lock.hash_lock();
        assert_eq!(hashed_lock1, hashed_lock2);
    }
}
