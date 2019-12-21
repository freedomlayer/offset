use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;

use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};
use ring::test::rand::FixedByteRandom;

use proto::crypto::{InvoiceId, PaymentId, PlainLock, PrivateKey, RandValue, Salt, Uid};

pub trait CryptoRandom: SecureRandom + Sync + Send {}

pub struct RngContainer<R> {
    arc_rng: Arc<R>,
}

impl<R> RngContainer<R> {
    pub fn new(rng: R) -> RngContainer<R> {
        RngContainer {
            arc_rng: Arc::new(rng),
        }
    }
}

impl<R> Clone for RngContainer<R> {
    fn clone(&self) -> Self {
        RngContainer {
            arc_rng: self.arc_rng.clone(),
        }
    }
}

impl<R: SecureRandom> SecureRandom for RngContainer<R> {
    fn fill(&self, dest: &mut [u8]) -> Result<(), Unspecified> {
        (*self.arc_rng).fill(dest)
    }
}

impl<R: SecureRandom> CryptoRandom for RngContainer<R> where R: Sync + Send {}
impl CryptoRandom for FixedByteRandom {}

impl<R> Deref for RngContainer<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &*self.arc_rng
    }
}

pub type OffstSystemRandom = RngContainer<SystemRandom>;

/// Returns a secure cryptographic random generator
pub fn system_random() -> OffstSystemRandom {
    RngContainer::new(SystemRandom::new())
}

/// `RandValuesStore` is a storage and generation structure of random values.
/// Those random values are used for cryptographic time.
/// A new random value is generated every `rand_value_ticks` time ticks.
/// There is only room for `num_rand_values` random values, so the creation of
/// new random values causes the deletion of old random values.
pub struct RandValuesStore {
    rand_values: VecDeque<RandValue>,
    ticks_left_to_next_rand_value: usize,
    rand_value_ticks: usize,
}

impl RandValuesStore {
    pub fn new<R: CryptoRandom>(
        crypt_rng: &R,
        rand_value_ticks: usize,
        num_rand_values: usize,
    ) -> Self {
        let rand_values = (0..num_rand_values)
            .map(|_| RandValue::rand_gen(crypt_rng))
            .collect::<VecDeque<RandValue>>();

        if num_rand_values == 0 {
            panic!("We require that num_rand_values > 0");
        }

        RandValuesStore {
            rand_values,
            ticks_left_to_next_rand_value: rand_value_ticks,
            rand_value_ticks,
        }
    }

    /// Check if we have a given rand_value.
    #[inline]
    pub fn contains(&self, x: &RandValue) -> bool {
        self.rand_values.contains(x)
    }

    /// Apply a time tick over the store.
    /// If enough time ticks have occurred, a new rand value will be generated.
    #[inline]
    pub fn time_tick<R: CryptoRandom>(&mut self, crypt_rng: &R) {
        self.ticks_left_to_next_rand_value -= 1;
        if self.ticks_left_to_next_rand_value == 0 {
            self.ticks_left_to_next_rand_value = self.rand_value_ticks;

            // Remove the oldest rand value:
            self.rand_values.pop_front();
            // Insert a new rand value:
            self.rand_values.push_back(RandValue::rand_gen(crypt_rng));
        }
    }

    /// Get the last random value that was generated.
    #[inline]
    pub fn last_rand_value(&self) -> RandValue {
        match self.rand_values.back() {
            None => unreachable!(),
            Some(rand_value) => rand_value.clone(),
        }
    }
}

pub trait RandGen: Sized {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self;
}

// TODO: Possibly use a macro here:
impl RandGen for Salt {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for InvoiceId {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for RandValue {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for PlainLock {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for Uid {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for PaymentId {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for PrivateKey {
    fn rand_gen(crypt_rng: &impl CryptoRandom) -> Self {
        PrivateKey::from(&ring::signature::Ed25519KeyPair::generate_pkcs8(crypt_rng).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::DummyRandom;
    use super::*;

    #[test]
    fn test_rand_values_store() {
        let rng = DummyRandom::new(&[1, 2, 3, 4, 5]);

        // Generate some unrelated rand value:
        let rand_value0 = RandValue::rand_gen(&rng);

        let mut rand_values_store = RandValuesStore::new(&rng, 50, 5);
        let rand_value = rand_values_store.last_rand_value();

        for _ in 0..(5 * 50) {
            assert!(rand_values_store.contains(&rand_value));
            assert!(!rand_values_store.contains(&rand_value0));
            rand_values_store.time_tick(&rng);
        }

        assert!(!rand_values_store.contains(&rand_value));
        assert!(!rand_values_store.contains(&rand_value0));
    }
}
