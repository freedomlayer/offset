use rand::{self, rngs::StdRng, RngCore};

use crate::error::CryptoError;
use crate::rand::CryptoRandom;

#[derive(Debug, Clone)]
pub struct DummyRandom {
    inner: StdRng,
}

impl DummyRandom {
    pub fn new(seed: &[u8]) -> Self {
        let mut rng_seed: [u8; 32] = [0; 32];
        // We copy as many seed bytes as we have as seed into rng_seed
        // If seed.len() > 32, clone_from_slice will panic.
        rng_seed[..seed.len()].clone_from_slice(seed);
        let rng = rand::SeedableRng::from_seed(rng_seed);

        DummyRandom { inner: rng }
    }
}

impl CryptoRandom for DummyRandom {
    fn fill(&mut self, dest: &mut [u8]) -> Result<(), CryptoError> {
        self.inner.fill_bytes(dest);
        Ok(())
    }
}
