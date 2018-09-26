#![allow(unused)]
use crypto::identity::PublicKey;

pub enum InitConnection {
    Listen,
    // remote side wants to accept a connection from public_key
    Accept(PublicKey),
    // remote side wants to connect to public_key
    Connect(PublicKey),
}

pub enum RelayListenIn {
    KeepAlive,
    RejectConnection(PublicKey),
}

pub enum RelayListenOut {
    KeepAlive,
    IncomingConnection(PublicKey),
}

pub enum TunnelMessage {
    KeepAlive,
    Message(Vec<u8>),
}
