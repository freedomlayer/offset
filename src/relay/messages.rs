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

#[derive(Debug, PartialEq, Eq)]
pub enum RelayListenIn {
    KeepAlive,
    RejectConnection(PublicKey),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RelayListenOut {
    KeepAlive,
    IncomingConnection(PublicKey),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TunnelMessage {
    KeepAlive,
    Message(Vec<u8>),
}
