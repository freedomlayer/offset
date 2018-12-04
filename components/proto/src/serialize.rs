use std::io;
use capnp;

#[derive(Debug)]
pub enum SerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
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
