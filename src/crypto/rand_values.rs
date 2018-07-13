use std::collections::VecDeque;

use ring::rand::SecureRandom;

pub const RAND_VALUE_LEN: usize = 16;

define_fixed_bytes!(RandValue, RAND_VALUE_LEN);

impl RandValue {
    pub fn new<R: SecureRandom>(crypt_rng: &R) -> Self {
        let mut rand_value = RandValue([0; RAND_VALUE_LEN]);
        crypt_rng.fill(&mut rand_value.0).unwrap();
        rand_value
    }
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
    pub fn new<R: SecureRandom>(
        crypt_rng: &R,
        rand_value_ticks: usize,
        num_rand_values: usize,
    ) -> Self {
        let rand_values = (0..num_rand_values)
            .map(|_| RandValue::new(crypt_rng))
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
    pub fn time_tick<R: SecureRandom>(&mut self, crypt_rng: &R) {
        self.ticks_left_to_next_rand_value -= 1;
        if self.ticks_left_to_next_rand_value == 0 {
            self.ticks_left_to_next_rand_value = self.rand_value_ticks;

            // Remove the oldest rand value:
            self.rand_values.pop_front();
            // Insert a new rand value:
            self.rand_values.push_back(RandValue::new(crypt_rng));
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::dummy_random;

    #[test]
    fn test_rand_values_store() {
        let rng = dummy_random(&[1, 2, 3, 4, 5]);

        // Generate some unrelated rand value:
        let rand_value0 = RandValue::new(&rng);

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
