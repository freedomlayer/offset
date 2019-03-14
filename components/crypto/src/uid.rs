use std::fmt;
use ring::rand::SecureRandom;

pub const UID_LEN: usize = 16;

// An Universally Unique Identifier (UUID).
define_fixed_bytes!(Uid, UID_LEN);

impl Copy for Uid {}

impl Uid {
    /// Creates a random `Uuid`.
    pub fn new<R: SecureRandom>(rng: &R) -> Uid {
        let mut uuid = Uid([0; UID_LEN]);
        rng.fill(&mut uuid.0).unwrap();
        uuid
    }

    /// Formatting for `Debug` and `Display`.
    fn format(&self) -> String {
        let upper_hex = self.as_ref()
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>();

        upper_hex.join("")
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}
