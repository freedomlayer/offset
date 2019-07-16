use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

#[capnp_conv(crate::keepalive_capnp::ka_message)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum KaMessage {
    KeepAlive,
    Message(Vec<u8>),
}
