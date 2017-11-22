extern crate ring;
use self::ring::rand::SecureRandom;

const UID_LEN: usize = 16;

#[derive(Debug, PartialEq, Clone)]
pub struct Uid([u8; UID_LEN]);


pub struct UidGenerator<R> {
    rng: R,
}

impl<R: SecureRandom> UidGenerator<R> {
    pub fn new(rng: R) -> Self {
        UidGenerator {
            rng,
        }
    }

    /// Generate a random Uid.
    pub fn gen_uid(&mut self) -> Uid {
        let mut uid = Uid([0; UID_LEN]);
        self.rng.fill(&mut uid.0);
        uid
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use self::ring::test::rand::FixedByteRandom;

    #[test]
    fn test_gen_uid() {
        let rng1 = FixedByteRandom { byte: 0x3 };
        let mut uid_gen1 = UidGenerator::new(rng1);

        let uid1 = uid_gen1.gen_uid();
        assert_eq!(uid1.0.len(), UID_LEN);

        let rng2 = FixedByteRandom { byte: 0x4 };
        let mut uid_gen2 = UidGenerator::new(rng2);

        let uid2 = uid_gen2.gen_uid();
        assert_ne!(uid1,uid2);

    }
}
