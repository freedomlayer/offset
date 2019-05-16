use ring::rand::SecureRandom;
use std::fmt;

pub const PAYMENT_ID_LEN: usize = 16;

define_fixed_bytes!(PaymentId, PAYMENT_ID_LEN);

impl Copy for PaymentId {}

impl PaymentId {
    pub fn new<R: SecureRandom>(rng: &R) -> PaymentId {
        let mut payment_id = PaymentId([0; PAYMENT_ID_LEN]);
        rng.fill(&mut payment_id.0).unwrap();
        payment_id
    }

    /// Formatting for `Debug` and `Display`.
    fn format(&self) -> String {
        let upper_hex = self
            .as_ref()
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>();

        upper_hex.join("")
    }
}

impl fmt::Display for PaymentId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}
