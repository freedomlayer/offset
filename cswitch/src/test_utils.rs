extern crate rand;
extern crate ring;

use std::cell::UnsafeCell;
use self::rand::{StdRng, Rng};

pub struct DummyRandom<R> {
    rng: UnsafeCell<R>,
}

impl DummyRandom<StdRng> {
    pub fn new(seed: &[usize]) -> Self {
        let rng_seed: &[_] = seed;
        let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);

        // Note: We use UnsafeCell here, because of the signature of the function fill.
        // It takes &self, it means that we can not change internal state with safe rust.
        DummyRandom {
            rng: UnsafeCell::new(rng), 
        }
    }
}

impl<R: Rng> ring::rand::SecureRandom for DummyRandom<R> {
    fn fill(&self, dest: &mut [u8]) -> Result<(), ring::error::Unspecified> {
        let rng = self.rng.get();
        unsafe {(*rng).fill_bytes(dest) };
        Ok(())
    }
}
