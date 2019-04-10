use crate::net::messages::NetAddressError;
use capnp;
use std::io;

#[derive(Debug, From)]
pub enum SerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
    NetAddressError(NetAddressError),
}
