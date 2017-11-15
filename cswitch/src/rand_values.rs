extern crate rand;

use std::collections::VecDeque;
use self::rand::Rng;

const RAND_VALUE_LEN: usize = 16;

#[derive(Clone, PartialEq)]
pub struct RandValue([u8; RAND_VALUE_LEN]);

impl RandValue {
    fn new<R: Rng>(crypt_rng: &mut R) -> Self {
        let mut rand_value = RandValue([0; RAND_VALUE_LEN]);
        crypt_rng.fill_bytes(&mut rand_value.0);
        rand_value
    }
}

/// RandValuesStore is a storage and generation structure of random values. Those random values
/// are used for cryptographic time. A new random value is generated every rand_value_ticks time
/// ticks. There is only room for num_rand_values random values, so the creation of new random
/// values causes the deletion of old random values.
pub struct RandValuesStore<R> {
    crypt_rng: R,
    rand_values: VecDeque<RandValue>,
    ticks_left_to_next_rand_value: usize,
    rand_value_ticks: usize,
}


impl<R: Rng> RandValuesStore<R> {
    pub fn new(mut crypt_rng: R, rand_value_ticks: usize, num_rand_values: usize) -> Self {
        let rand_values = (0 .. num_rand_values).map(|_| RandValue::new(&mut crypt_rng))
            .collect::<VecDeque<RandValue>>();

        if num_rand_values == 0 {
            panic!("We require that num_rand_values > 0");
        }

        RandValuesStore {
            crypt_rng,
            rand_values,
            ticks_left_to_next_rand_value: rand_value_ticks,
            rand_value_ticks,
        }
    }

    /// Check if we have a given rand_value.
    pub fn check_rand_value(&self, rand_value: &RandValue) -> bool {
        match self.rand_values.iter().find(
                |&iter_rand_value| iter_rand_value == rand_value) {
            Some(_) => true,
            None => false,
        }
    }

    /// Apply a time tick over the store.
    /// If enough time ticks have occured, a new rand value will be generated.
    pub fn time_tick(&mut self) {
        self.ticks_left_to_next_rand_value -= 1;
        if self.ticks_left_to_next_rand_value == 0 {
            self.ticks_left_to_next_rand_value = self.rand_value_ticks;

            // Remove the oldest rand value:
            self.rand_values.pop_front();
            // Insert a new rand value:
            self.rand_values.push_back(RandValue::new(&mut self.crypt_rng));
        }
    }

    /// Get the last random value that was generated.
    pub fn last_rand_value(&self) -> RandValue {
        match self.rand_values.back() {
            None => unreachable!(),
            Some(rand_value) => rand_value.clone(),
        }
    }
}
