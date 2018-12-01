use std::io;
use capnp;
use capnp::serialize_packed;
use keepalive_capnp;

use crate::serialize::SerializeError;
use super::messages::KaMessage;


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

pub fn deserialize_ka_message(data: &[u8]) -> Result<KaMessage, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<keepalive_capnp::ka_message::Reader>()?;

    match msg.which() {
        Ok(keepalive_capnp::ka_message::KeepAlive(())) => 
           Ok(KaMessage::KeepAlive),
        Ok(keepalive_capnp::ka_message::Message(opt_message_reader)) =>
            Ok(KaMessage::Message(Vec::from(opt_message_reader?))),
        Err(e) => Err(SerializeError::NotInSchema(e)),
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
