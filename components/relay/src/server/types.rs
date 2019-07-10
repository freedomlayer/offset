use proto::crypto::PublicKey;

pub struct IncomingListen<M, K> {
    pub receiver: M,
    pub sender: K,
}

pub struct IncomingAccept<M, K> {
    pub receiver: M,
    pub sender: K,
    pub accept_public_key: PublicKey,
}

pub struct IncomingConnect<M, K> {
    pub receiver: M,
    pub sender: K,
    pub connect_public_key: PublicKey,
}

pub enum IncomingConnInner<ML, KL, MA, KA, MC, KC> {
    Listen(IncomingListen<ML, KL>),
    Accept(IncomingAccept<MA, KA>),
    Connect(IncomingConnect<MC, KC>),
}

pub struct IncomingConn<ML, KL, MA, KA, MC, KC> {
    pub public_key: PublicKey,
    pub inner: IncomingConnInner<ML, KL, MA, KA, MC, KC>,
}
