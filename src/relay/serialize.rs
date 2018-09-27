#![allow(unused)]
use std::io;
use std::convert::TryFrom;
use capnp;
use capnp::serialize_packed;
use crypto::identity::PublicKey;
use utils::capnp_custom_int::{read_custom_u_int256, 
                                write_custom_u_int256};

use relay_capnp;

use super::messages::{InitConnection, RelayListenIn, 
    RelayListenOut, TunnelMessage};

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

pub fn serialize_init_connection(init_connection: &InitConnection) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<relay_capnp::init_connection::Builder>();

    match init_connection {
        InitConnection::Listen => msg.set_listen(()),
        InitConnection::Accept(public_key) => {
            let mut accept = msg.init_accept();
            write_custom_u_int256(&public_key, &mut accept);
        }
        InitConnection::Connect(public_key) => {
            let mut connect = msg.init_connect();
            write_custom_u_int256(&public_key, &mut connect);
        },
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_init_connection(data: &[u8]) -> Result<InitConnection, RelaySerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<relay_capnp::init_connection::Reader>()?;

    match msg.which() {
        Ok(relay_capnp::init_connection::Listen(())) => 
           Ok(InitConnection::Listen),
        Ok(relay_capnp::init_connection::Accept(public_key)) => {
            let public_key_bytes = &read_custom_u_int256(&(public_key?));
            let public_key = PublicKey::try_from(&public_key_bytes[..]).unwrap();
            Ok(InitConnection::Accept(public_key))
        },
        Ok(relay_capnp::init_connection::Connect(public_key)) => {
            let public_key_bytes = &read_custom_u_int256(&(public_key?));
            let public_key = PublicKey::try_from(&public_key_bytes[..]).unwrap();
            Ok(InitConnection::Connect(public_key))
        },
        Err(e) => Err(RelaySerializeError::NotInSchema(e)),
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
    use crypto::identity::PUBLIC_KEY_LEN;

    #[test]
    fn test_serialize_init_connection() {
        let msg = InitConnection::Listen;
        let serialized = serialize_init_connection(&msg);
        let msg2 = deserialize_init_connection(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);

        let public_key = PublicKey::try_from(&[0x02u8; PUBLIC_KEY_LEN][..]).unwrap();
        let msg = InitConnection::Accept(public_key);
        let serialized = serialize_init_connection(&msg);
        let msg2 = deserialize_init_connection(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);

        let public_key = PublicKey::try_from(&[0x02u8; PUBLIC_KEY_LEN][..]).unwrap();
        let msg = InitConnection::Connect(public_key);
        let serialized = serialize_init_connection(&msg);
        let msg2 = deserialize_init_connection(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);

    }

    #[test]
    fn test_serialize_tunnel_message() {
        let msg = TunnelMessage::KeepAlive;
        let serialized = serialize_tunnel_message(&msg);
        let msg2 = deserialize_tunnel_message(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);

        let msg = TunnelMessage::Message(b"Hello world".to_vec());
        let serialized = serialize_tunnel_message(&msg);
        let msg2 = deserialize_tunnel_message(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }
}
