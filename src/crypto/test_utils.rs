use std::cell::RefCell;

use rand::{self, Rng, StdRng};
use ring::{error::Unspecified, rand::SecureRandom};
use ring::test::rand::NotRandom;

pub fn dummy_random(seed: &[usize]) -> impl SecureRandom {
    let rng: StdRng = rand::SeedableRng::from_seed(seed);
    let rng = RefCell::new(rng);
    NotRandom(move |dest: &mut [u8]| -> Result<(), Unspecified> {
        rng.borrow_mut().fill_bytes(dest);
        Ok(())
    })
}
