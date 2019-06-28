use std::io;

use capnp;

use crate::net::messages::NetAddressError;

use derive_more::From;

#[derive(Debug, From)]
pub enum SerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
    NetAddressError(NetAddressError),
}
