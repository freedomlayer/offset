extern crate ring;
use self::ring::rand::SecureRandom;

const UID_LEN: usize = 16;

#[derive(Debug, PartialEq, Clone)]
pub struct Uid([u8; UID_LEN]);



pub fn gen_uid<R: SecureRandom>(rng: &R) -> Uid {
    let mut uid = Uid([0; UID_LEN]);
    rng.fill(&mut uid.0).unwrap();
    uid
}


#[cfg(test)]
mod tests {
    use super::*;
    use self::ring::test::rand::FixedByteRandom;

    #[test]
    fn test_gen_uid() {
        let rng1 = FixedByteRandom { byte: 0x3 };

        let uid1 = gen_uid(&rng1);
        assert_eq!(uid1.0.len(), UID_LEN);

        let rng2 = FixedByteRandom { byte: 0x4 };

        let uid2 = gen_uid(&rng2);
        assert_ne!(uid1,uid2);

    }
}
