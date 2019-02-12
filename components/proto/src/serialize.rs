use std::io;
use capnp;
use crate::net::messages::NetAddressError;

#[derive(Debug)]
pub enum SerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
    NetAddressError(NetAddressError),
}


impl From<capnp::Error> for SerializeError {
    fn from(e: capnp::Error) -> SerializeError {
        SerializeError::CapnpError(e)
    }
}

impl From<capnp::NotInSchema> for SerializeError {
    fn from(e: capnp::NotInSchema) -> SerializeError {
        SerializeError::NotInSchema(e)
    }
}

impl From<io::Error> for SerializeError {
    fn from(e: io::Error) -> SerializeError {
        SerializeError::IoError(e)
    }
}

impl From<NetAddressError> for SerializeError {
    fn from(e: NetAddressError) -> SerializeError {
        SerializeError::NetAddressError(e)
    }
}
