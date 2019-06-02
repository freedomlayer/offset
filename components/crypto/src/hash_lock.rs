use ring::digest::{digest, SHA512_256};
use ring::rand::SecureRandom;

// TODO: Use bcrypt instead here

pub const PLAIN_LOCK_LEN: usize = 32;
pub const HASHED_LOCK_LEN: usize = 32;

define_fixed_bytes!(PlainLock, PLAIN_LOCK_LEN);
define_fixed_bytes!(HashedLock, HASHED_LOCK_LEN);

impl PlainLock {
    /// Randomly generate a new PlainLock
    pub fn new<R: SecureRandom>(rng: &R) -> Self {
        let mut plain_lock = PlainLock([0; PLAIN_LOCK_LEN]);
        rng.fill(&mut plain_lock.0).unwrap();
        plain_lock
    }

    pub fn hash(&self) -> HashedLock {
        let mut inner = [0x00; HASHED_LOCK_LEN];
        let digest_res = digest(&SHA512_256, &self.0);
        inner.copy_from_slice(digest_res.as_ref());
        HashedLock(inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_lock_basic() {
        let plain_lock = PlainLock::from(&[1u8; PLAIN_LOCK_LEN]);
        let hashed_lock1 = plain_lock.hash();
        let hashed_lock2 = plain_lock.hash();
        assert_eq!(hashed_lock1, hashed_lock2);
    }
}
