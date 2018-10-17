#![allow(unused)]
use crypto::identity::PublicKey;

#[derive(Debug, PartialEq, Eq)]
pub enum InitConnection {
    Listen,
    // remote side wants to accept a connection from public_key
    Accept(PublicKey),
    // remote side wants to connect to public_key
    Connect(PublicKey),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RejectConnection(pub PublicKey);

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RelayListenIn {
    KeepAlive,
    RejectConnection(RejectConnection),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IncomingConnection(pub PublicKey);

#[derive(Debug, PartialEq, Eq)]
pub enum RelayListenOut {
    KeepAlive,
    IncomingConnection(IncomingConnection),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TunnelMessage {
    KeepAlive,
    Message(Vec<u8>),
}
