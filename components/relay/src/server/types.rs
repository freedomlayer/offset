use common::conn::{ConnPair, ConnPairVec};

use proto::crypto::PublicKey;
use proto::relay::messages::{IncomingConnection, RejectConnection};

pub struct IncomingListen {
    pub conn_pair: ConnPair<IncomingConnection, RejectConnection>,
}

pub struct IncomingAccept {
    pub accept_public_key: PublicKey,
    pub conn_pair: ConnPairVec,
}

pub struct IncomingConnect {
    pub connect_public_key: PublicKey,
    pub conn_pair: ConnPairVec,
}

pub enum IncomingConnInner {
    Listen(IncomingListen),
    Accept(IncomingAccept),
    Connect(IncomingConnect),
}

pub struct IncomingConn {
    pub public_key: PublicKey,
    pub inner: IncomingConnInner,
}
