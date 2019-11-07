use crypto::rand::{system_random, RandGen};

use proto::crypto::{InvoiceId, PaymentId, Uid};

// TODO: Use Gen trait here instead?

/// Generate a random uid
pub fn gen_uid() -> Uid {
    // Obtain secure cryptographic random:
    let rng = system_random();

    Uid::rand_gen(&rng)
}

/// Generate a random InvoiceId:
pub fn gen_invoice_id() -> InvoiceId {
    // Obtain secure cryptographic random:
    let rng = system_random();

    InvoiceId::rand_gen(&rng)
}

/// Generate a random PaymentId:
pub fn gen_payment_id() -> PaymentId {
    // Obtain secure cryptographic random:
    let rng = system_random();

    PaymentId::rand_gen(&rng)
}
