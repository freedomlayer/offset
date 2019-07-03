use crate::crypto::PublicKey;

#[derive(Debug, PartialEq, Eq)]
pub enum InitConnection {
    Listen,
    // remote side wants to accept a connection from public_key
    Accept(PublicKey),
    // remote side wants to connect to public_key
    Connect(PublicKey),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RejectConnection {
    pub public_key: PublicKey,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IncomingConnection {
    pub public_key: PublicKey,
}
