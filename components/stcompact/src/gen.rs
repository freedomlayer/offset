use crypto::rand::{CryptoRandom, RandGen};

use app::common::{PrivateKey, Uid};

pub trait GenUid {
    /// Generate a Uid
    fn gen_uid(&mut self) -> Uid;
}

/*
pub trait GenPaymentId {
    /// Generate a PaymentId
    fn gen_payment_id(&mut self) -> PaymentId;
}
*/

pub trait GenPrivateKey {
    /// Generate private key
    fn gen_private_key(&mut self) -> PrivateKey;
}

// TODO: Find a way to eliminate this shim.
// Possibly have all the crates use traits like GenUid, GenPrivateKey, GenNonce etc?
/// A wrapper over a random generator that implements
/// GenUid and GenPrivateKey
pub struct GenCryptoRandom<R>(pub R);

impl<R> GenPrivateKey for GenCryptoRandom<R>
where
    R: CryptoRandom,
{
    fn gen_private_key(&mut self) -> PrivateKey {
        PrivateKey::rand_gen(&mut self.0)
    }
}

impl<R> GenUid for GenCryptoRandom<R>
where
    R: CryptoRandom,
{
    fn gen_uid(&mut self) -> Uid {
        Uid::rand_gen(&mut self.0)
    }
}
