use app::common::{PaymentId, Uid};

pub trait GenId {
    /// Generate a Uid
    fn gen_uid(&mut self) -> Uid;

    /// Generate a PaymentId
    fn gen_payment_id(&mut self) -> PaymentId;
}
