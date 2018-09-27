#![allow(unused)]
use std::io;
use std::convert::TryFrom;
use capnp;
use capnp::serialize_packed;

use relay_capnp;

use super::messages::{TunnelMessage};

#[derive(Debug)]
pub enum RelaySerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}


impl From<capnp::Error> for RelaySerializeError {
    fn from(e: capnp::Error) -> RelaySerializeError {
        RelaySerializeError::CapnpError(e)
    }
}

impl From<io::Error> for RelaySerializeError {
    fn from(e: io::Error) -> RelaySerializeError {
        RelaySerializeError::IoError(e)
    }
}

pub fn serialize_tunnel_message(tunnel_message: &TunnelMessage) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<relay_capnp::tunnel_message::Builder>();

    match tunnel_message {
        TunnelMessage::KeepAlive => msg.set_keep_alive(()),
        TunnelMessage::Message(message) => msg.set_message(message),
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_tunnel_message(data: &[u8]) -> Result<TunnelMessage, RelaySerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<relay_capnp::tunnel_message::Reader>()?;

    match msg.which() {
        Ok(relay_capnp::tunnel_message::KeepAlive(())) => 
           Ok(TunnelMessage::KeepAlive),
        Ok(relay_capnp::tunnel_message::Message(message)) => 
           Ok(TunnelMessage::Message(message?.to_vec())),
        Err(e) => Err(RelaySerializeError::NotInSchema(e)),
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_tunnel_message() {
        let msg = TunnelMessage::Message(b"Hello world".to_vec());
        let serialized = serialize_tunnel_message(&msg);
        let msg2 = deserialize_tunnel_message(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }
}
