///! A rough implementation of UUID v4.

use std::fmt;
use ring::rand::SecureRandom;

const UUID_LEN: usize = 16;

/// A Universally Unique Identifier (UUID).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid([u8; UUID_LEN]);

impl Uuid {
    /// Creates a random `Uuid`.
    pub fn new<R: SecureRandom>(rng: &R) -> Uuid {
        let mut uuid = Uuid([0; UUID_LEN]);
        rng.fill(&mut uuid.0).unwrap();
        uuid
    }

    /// Returns an array of 16 bytes containing the `Uuid` data.
    pub fn as_bytes(&self) -> &[u8; UUID_LEN] {
        &self.0
    }

    /// Formatting for `Debug` and `Display`.
    fn format(&self) -> String {
        let upper_hex = self.as_bytes().iter().map(|byte| {
            format!("{:02X}", byte)
        }).collect::<Vec<_>>();

        upper_hex.join("")
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

impl fmt::Display for Uuid {
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
        let uuid1 = Uuid::new(&rng1);

        assert_eq!(uuid1.0.len(), UUID_LEN);
        assert_eq!(uuid1.as_bytes(), &[0x3; UUID_LEN]);

        let rng2 = FixedByteRandom { byte: 0x4 };
        let uuid2 = Uuid::new(&rng2);

        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn format() {
        let rng = FixedByteRandom { byte: 0x03 };
        let uuid = Uuid::new(&rng);

        let debug = format!("{:?}", uuid);
        let display = format!("{}", uuid);

        let expected = "03030303030303030303030303030303".to_owned();
        assert_eq!(expected.len(), 32);

        assert_eq!(debug, expected);
        assert_eq!(display, expected);
    }
}
