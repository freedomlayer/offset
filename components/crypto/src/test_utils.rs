use std::cell::RefCell;

use rand::{self, Rng, StdRng};
use ring::{error::Unspecified, rand::SecureRandom};

pub struct DummyRandom<R> {
    rng: RefCell<R>,
}

impl DummyRandom<StdRng> {
    pub fn new(seed: &[u8]) -> Self {
        let mut rng_seed: [u8; 32] = [0; 32];
        // We copy as many seed bytes as we have as seed into rng_seed
        // If seed.len() > 32, clone_from_slice will panic.
        rng_seed[.. seed.len()].clone_from_slice(seed);
        let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);

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
