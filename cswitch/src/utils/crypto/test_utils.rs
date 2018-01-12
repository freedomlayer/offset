use std::cell::RefCell;

use rand::{self, Rng, StdRng};
use ring::{error::Unspecified, rand::SecureRandom};

pub struct DummyRandom<R> {
    rng: RefCell<R>,
}

impl DummyRandom<StdRng> {
    pub fn new(seed: &[usize]) -> Self {
        let rng_seed: &[_] = seed;
        let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);

        // Note: We use UnsafeCell here, because of the signature of the function fill.
        // It takes &self, it means that we can not change internal state with safe rust.
        DummyRandom {
            rng: RefCell::new(rng),
        }
    }
}

impl<R: Rng> SecureRandom for DummyRandom<R> {
    fn fill(&self, dest: &mut [u8]) -> Result<(), Unspecified> {
        self.rng.borrow_mut().fill_bytes(dest);
        Ok(())
    }
}
