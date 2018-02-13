use std::fmt;
use ring::rand::SecureRandom;

const UID_LEN: usize = 16;

/// A Universally Unique Identifier (UUID).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uid([u8; UID_LEN]);

impl Uid {
    /// Creates a random `Uuid`.
    pub fn new<R: SecureRandom>(rng: &R) -> Uid {
        let mut uuid = Uid([0; UID_LEN]);
        rng.fill(&mut uuid.0).unwrap();
        uuid
    }

    /// Returns an array of 16 bytes containing the `Uuid` data.
    pub fn as_bytes(&self) -> &[u8; UID_LEN] {
        &self.0
    }

    /// Formatting for `Debug` and `Display`.
    fn format(&self) -> String {
        let upper_hex = self.as_bytes()
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>();

        upper_hex.join("")
    }
}

impl fmt::Debug for Uid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn basic() {
        let rng1 = FixedByteRandom { byte: 0x03 };
        let uid1 = Uid::new(&rng1);

        assert_eq!(uid1.0.len(), UID_LEN);
        assert_eq!(uid1.as_bytes(), &[0x3; UID_LEN]);

        let rng2 = FixedByteRandom { byte: 0x4 };
        let uid2 = Uid::new(&rng2);

        assert_ne!(uid1, uid2);
    }

    #[test]
    fn format() {
        let rng = FixedByteRandom { byte: 0x03 };
        let uid = Uid::new(&rng);

        let debug = format!("{:?}", uid);
        let display = format!("{}", uid);

        let expected = "03030303030303030303030303030303".to_owned();
        assert_eq!(expected.len(), 32);

        assert_eq!(debug, expected);
        assert_eq!(display, expected);
    }
}
