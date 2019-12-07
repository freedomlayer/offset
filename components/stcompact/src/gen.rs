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
