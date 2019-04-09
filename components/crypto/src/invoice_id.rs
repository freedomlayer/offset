use ring::rand::SecureRandom;
use std::fmt;

pub const INVOICE_ID_LEN: usize = 32;

// An invoice identifier
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

impl InvoiceId {
    /// Creates a random `InvoiceId`.
    pub fn new<R: SecureRandom>(rng: &R) -> InvoiceId {
        let mut invoice_id = InvoiceId([0; INVOICE_ID_LEN]);
        rng.fill(&mut invoice_id.0).unwrap();
        invoice_id
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

impl fmt::Display for InvoiceId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}
