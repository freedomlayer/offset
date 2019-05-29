use crypto::crypto_rand::system_random;
use crypto::uid::Uid;
pub use crypto::uid::UID_LEN;

use crypto::invoice_id::InvoiceId;
use crypto::payment_id::PaymentId;

use node::connect::{node_connect, NodeConnection};

/// Generate a random uid
pub fn gen_uid() -> Uid {
    // Obtain secure cryptographic random:
    let rng = system_random();

    Uid::new(&rng)
}

/// Generate a random InvoiceId:
pub fn gen_invoice_id() -> InvoiceId {
    // Obtain secure cryptographic random:
    let rng = system_random();

    InvoiceId::new(&rng)
}

/// Generate a random PaymentId:
pub fn gen_payment_id() -> PaymentId {
    // Obtain secure cryptographic random:
    let rng = system_random();

    PaymentId::new(&rng)
}
