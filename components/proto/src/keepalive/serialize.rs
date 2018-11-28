use std::io;
use capnp;
use capnp::serialize_packed;
use keepalive_capnp;

use super::messages::KaMessage;


#[derive(Debug)]
pub enum KeepAliveSerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}


impl From<capnp::Error> for KeepAliveSerializeError {
    fn from(e: capnp::Error) -> KeepAliveSerializeError {
        KeepAliveSerializeError::CapnpError(e)
    }
}

impl From<capnp::NotInSchema> for KeepAliveSerializeError {
    fn from(e: capnp::NotInSchema) -> KeepAliveSerializeError {
        KeepAliveSerializeError::NotInSchema(e)
    }
}

impl From<io::Error> for KeepAliveSerializeError {
    fn from(e: io::Error) -> KeepAliveSerializeError {
        KeepAliveSerializeError::IoError(e)
    }
}


pub fn serialize_ka_message(ka_message: &KaMessage) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<keepalive_capnp::ka_message::Builder>();

    match ka_message {
        KaMessage::KeepAlive => msg.set_keep_alive(()),
        KaMessage::Message(message) => msg.set_message(message),
    };

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_ka_message(data: &[u8]) -> Result<KaMessage, KeepAliveSerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<keepalive_capnp::ka_message::Reader>()?;

    match msg.which() {
        Ok(keepalive_capnp::ka_message::KeepAlive(())) => 
           Ok(KaMessage::KeepAlive),
        Ok(keepalive_capnp::ka_message::Message(opt_message_reader)) =>
            Ok(KaMessage::Message(Vec::from(opt_message_reader?))),
        Err(e) => Err(KeepAliveSerializeError::NotInSchema(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_serialize_ka_message_message() {
        let ka_message = KaMessage::Message(vec![1,2,3,4,5]);
        let ser_data = serialize_ka_message(&ka_message);
        let ka_message2 = deserialize_ka_message(&ser_data).unwrap();
        assert_eq!(ka_message, ka_message2);
    }

    #[test]
    fn test_basic_serialize_ka_message_keepalive() {
        let ka_message = KaMessage::KeepAlive;
        let ser_data = serialize_ka_message(&ka_message);
        let ka_message2 = deserialize_ka_message(&ser_data).unwrap();
        assert_eq!(ka_message, ka_message2);

    }

}
