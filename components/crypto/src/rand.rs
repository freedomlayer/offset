use std::collections::VecDeque;

use rand::{rngs::OsRng, CryptoRng, RngCore};
use rand_core::impls;

use crate::error::CryptoError;

use proto::crypto::{InvoiceId, PaymentId, PlainLock, RandValue, Salt, Uid};

// TODO: Maybe we shouldn't have Sync + Send here as bounds?
pub trait CryptoRandom: RngCore + CryptoRng {
    fn fill(&mut self, dest: &mut [u8]) -> Result<(), CryptoError>;
}

#[derive(Debug, Clone)]
pub struct SystemRandom {
    inner: OsRng,
}

impl SystemRandom {
    pub fn new() -> Self {
        SystemRandom { inner: OsRng }
    }
}

impl RngCore for SystemRandom {
    fn next_u32(&mut self) -> u32 {
        impls::next_u32_via_fill(self)
    }

    fn next_u64(&mut self) -> u64 {
        impls::next_u64_via_fill(self)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        // Rely on inner random generator:
        self.inner.fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for SystemRandom {}

impl CryptoRandom for SystemRandom {
    fn fill(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        self.inner.fill_bytes(dest);
        Ok(())
    }
}

/// Returns a secure cryptographic random generator
pub fn system_random() -> SystemRandom {
    SystemRandom::new()
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
        crypt_rng: &mut R,
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
    pub fn time_tick<R: CryptoRandom>(&mut self, crypt_rng: &mut R) {
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
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self;
}

// TODO: Possibly use a macro here:
impl RandGen for Salt {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for InvoiceId {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for RandValue {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for PlainLock {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for Uid {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

impl RandGen for PaymentId {
    fn rand_gen(crypt_rng: &mut impl CryptoRandom) -> Self {
        let mut res = Self::default();
        crypt_rng.fill(&mut res).unwrap();
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_utils::DummyRandom;
    use super::*;

    #[test]
    fn test_rand_values_store() {
        let mut rng = DummyRandom::new(&[1, 2, 3, 4, 5]);

        // Generate some unrelated rand value:
        let rand_value0 = RandValue::rand_gen(&mut rng);

        let mut rand_values_store = RandValuesStore::new(&mut rng, 50, 5);
        let rand_value = rand_values_store.last_rand_value();

        for _ in 0..(5 * 50) {
            assert!(rand_values_store.contains(&rand_value));
            assert!(!rand_values_store.contains(&rand_value0));
            rand_values_store.time_tick(&mut rng);
        }

        assert!(!rand_values_store.contains(&rand_value));
        assert!(!rand_values_store.contains(&rand_value0));
    }
}
