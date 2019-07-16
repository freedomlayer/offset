use ring::digest::{digest, SHA512_256};

use proto::crypto::{HashedLock, PlainLock};

pub trait HashLock {
    /// Lock the plain hash
    fn hash_lock(&self) -> HashedLock;
}

impl HashLock for PlainLock {
    // TODO: Use bcrypt instead here (As suggested by @spolu)
    fn hash_lock(&self) -> HashedLock {
        let mut hashed_lock = HashedLock::default();
        let inner = &mut hashed_lock;
        let digest_res = digest(&SHA512_256, &self);
        inner.copy_from_slice(digest_res.as_ref());
        hashed_lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proto::crypto::{PlainLock, PLAIN_LOCK_LEN};

    #[test]
    fn test_hash_lock_basic() {
        let plain_lock = PlainLock::from(&[1u8; PLAIN_LOCK_LEN]);
        let hashed_lock1 = plain_lock.hash_lock();
        let hashed_lock2 = plain_lock.hash_lock();
        assert_eq!(hashed_lock1, hashed_lock2);
    }
}
