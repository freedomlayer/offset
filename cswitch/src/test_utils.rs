extern crate rand;
extern crate ring;

use self::rand::{StdRng, Rng};

pub struct DummyRandom<'a> {
    seed: &'a [usize],
}

impl<'a> DummyRandom<'a> {
    pub fn new(seed: &'a [usize]) -> Self {
        DummyRandom {
            seed,
        }
    }
}

impl<'a> ring::rand::SecureRandom for DummyRandom<'a> {
    fn fill(&self, dest: &mut [u8]) -> Result<(), ring::error::Unspecified> {
        let rng_seed: &[_] = self.seed;
        let mut rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
        rng.fill_bytes(dest);
        Ok(())
    }
}
