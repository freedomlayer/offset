use std::cell::RefCell;
use std::clone::Clone;
use std::sync::Mutex;

use rand::rngs::StdRng;
use rand::{self, RngCore};
use ring::{error::Unspecified, rand::SecureRandom};

use crate::rand::CryptoRandom;

pub struct DummyRandom {
    inner: Mutex<RefCell<StdRng>>,
}

impl Clone for DummyRandom {
    fn clone(&self) -> Self {
        let guard = self.inner.lock().unwrap();
        let rng = (*guard).clone();
        DummyRandom {
            inner: Mutex::new(rng),
        }
    }
}

impl DummyRandom {
    pub fn new(seed: &[u8]) -> Self {
        let mut rng_seed: [u8; 32] = [0; 32];
        // We copy as many seed bytes as we have as seed into rng_seed
        // If seed.len() > 32, clone_from_slice will panic.
        rng_seed[..seed.len()].clone_from_slice(seed);
        let rng = rand::SeedableRng::from_seed(rng_seed);

        DummyRandom {
            inner: Mutex::new(RefCell::new(rng)),
        }
    }
}

impl SecureRandom for DummyRandom {
    fn fill(&self, dest: &mut [u8]) -> Result<(), Unspecified> {
        let guard = self.inner.lock().unwrap();
        let ref_cell = &*guard;
        ref_cell.borrow_mut().fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRandom for DummyRandom {}
