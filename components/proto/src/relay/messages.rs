use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use crate::crypto::PublicKey;

#[capnp_conv(crate::relay_capnp::init_connection)]
#[derive(Debug, PartialEq, Eq)]
pub enum InitConnection {
    Listen,
    // remote side wants to accept a connection from public_key
    Accept(PublicKey),
    // remote side wants to connect to public_key
    Connect(PublicKey),
}

#[capnp_conv(crate::relay_capnp::reject_connection)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RejectConnection {
    pub public_key: PublicKey,
}

#[capnp_conv(crate::relay_capnp::incoming_connection)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IncomingConnection {
    pub public_key: PublicKey,
}
