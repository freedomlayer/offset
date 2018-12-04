use std::io;
use capnp;
use capnp::serialize_packed;
use crate::capnp_common::{write_public_key,
                        read_public_key};

use relay_capnp;

use super::messages::{InitConnection, 
    RejectConnection, IncomingConnection};

use crate::serialize::SerializeError;

pub fn serialize_init_connection(init_connection: &InitConnection) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<relay_capnp::init_connection::Builder>();

    match init_connection {
        InitConnection::Listen => msg.set_listen(()),
        InitConnection::Accept(public_key) => {
            let mut accept = msg.init_accept();
            write_public_key(&public_key, &mut accept);
        }
        InitConnection::Connect(public_key) => {
            let mut connect = msg.init_connect();
            write_public_key(&public_key, &mut connect);
        },
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_init_connection(data: &[u8]) -> Result<InitConnection, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<relay_capnp::init_connection::Reader>()?;

    match msg.which() {
        Ok(relay_capnp::init_connection::Listen(())) => 
           Ok(InitConnection::Listen),
        Ok(relay_capnp::init_connection::Accept(public_key)) => {
            let public_key = read_public_key(&(public_key?))?;
            Ok(InitConnection::Accept(public_key))
        },
        Ok(relay_capnp::init_connection::Connect(public_key)) => {
            let public_key = read_public_key(&(public_key?))?;
            Ok(InitConnection::Connect(public_key))
        },
        Err(e) => Err(SerializeError::NotInSchema(e)),
    }
}

pub fn serialize_reject_connection(reject_connection: &RejectConnection) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let msg = builder.init_root::<relay_capnp::reject_connection::Builder>();

    write_public_key(&reject_connection.public_key, &mut msg.init_public_key());

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_reject_connection(data: &[u8]) -> Result<RejectConnection, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<relay_capnp::reject_connection::Reader>()?;

    let public_key = read_public_key(&(msg.get_public_key()?))?;
    Ok(RejectConnection { public_key })
}

pub fn serialize_incoming_connection(incoming_connection: &IncomingConnection) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let msg = builder.init_root::<relay_capnp::incoming_connection::Builder>();

    write_public_key(&incoming_connection.public_key, &mut msg.init_public_key());

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_incoming_connection(data: &[u8]) -> Result<IncomingConnection, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<relay_capnp::incoming_connection::Reader>()?;

    let public_key = read_public_key(&(msg.get_public_key()?))?;
    Ok(IncomingConnection { public_key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::PUBLIC_KEY_LEN;
    use std::convert::TryFrom;
    use crypto::identity::PublicKey;

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
    fn test_serialize_reject_connection() {
        let public_key = PublicKey::try_from(&[0x55u8; PUBLIC_KEY_LEN][..]).unwrap();
        let msg = RejectConnection { public_key };
        let serialized = serialize_reject_connection(&msg);
        let msg2 = deserialize_reject_connection(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_serialize_incoming_connection() {
        let public_key = PublicKey::try_from(&[0x55u8; PUBLIC_KEY_LEN][..]).unwrap();
        let msg = IncomingConnection { public_key };
        let serialized = serialize_incoming_connection(&msg);
        let msg2 = deserialize_incoming_connection(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }
}

