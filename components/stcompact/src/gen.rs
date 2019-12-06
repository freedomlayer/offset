use app::common::{PaymentId, PrivateKey, Uid};

pub trait CompactGen {
    /// Generate a Uid
    fn gen_uid(&mut self) -> Uid;

    /// Generate a PaymentId
    fn gen_payment_id(&mut self) -> PaymentId;

    /// Generate private key
    fn gen_private_key(&mut self) -> PrivateKey;
}
