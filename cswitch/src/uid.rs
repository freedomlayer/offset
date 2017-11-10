extern crate rand;
use self::rand::Rng;

const UID_LEN: usize = 16;

#[derive(Debug, PartialEq)]
pub struct Uid([u8; UID_LEN]);


struct UidGenerator<R> {
    rng: R,
}

impl<R:Rng> UidGenerator<R> {
    fn new(rng: R) -> Self {
        UidGenerator {
            rng,
        }
    }

    /// Generate a random Uid.
    fn gen_uid(&mut self) -> Uid {
        let mut uid = Uid([0; UID_LEN]);
        self.rng.fill_bytes(&mut uid.0);
        uid
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use self::rand::StdRng;

    #[test]
    fn test_gen_uid() {
        let seed: &[_] = &[1,2,3,4,5];
        let rng: StdRng = rand::SeedableRng::from_seed(seed);
        let mut uid_gen = UidGenerator::new(rng);

        let uid1 = uid_gen.gen_uid();
        assert_eq!(uid1.0.len(), UID_LEN);
        let uid2 = uid_gen.gen_uid();
        assert_ne!(uid1,uid2);

    }
}
