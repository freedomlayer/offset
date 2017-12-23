use std::io;

use rand::{Rng, OsRng};
use bytes::Bytes;
use capnp::serialize_packed;

use crypto::rand_values::RandValue;
use crypto::dh::{Salt, DhPublicKey};
use crypto::identity::{PublicKey, Signature};

use channeler_capnp::{self, init_channel, exchange, encrypt_message, MessageType, /*message*/};

use super::{
    SchemaError,
    read_public_key,
    write_public_key,
    read_rand_value,
    write_rand_value,
    read_signature,
    write_signature,
    read_salt,
    write_salt,
    read_dh_public_key,
    write_dh_public_key,
};

/// Create and serialize a `Message` from given `content`, return the serialized message on success.
#[inline]
pub fn serialize_message(content: Bytes) -> Result<Bytes, SchemaError> {
    let mut builder = ::capnp::message::Builder::new_default();

    {
        let mut message = builder.init_root::<channeler_capnp::message::Builder>();
        let msg_content = message.borrow().init_content(content.len() as u32);
        // Optimize: Can we avoid copy here?
        msg_content.copy_from_slice(content.as_ref());
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;

    Ok(Bytes::from(serialized_msg))
}

/// Deserialize `Message` from `buffer`, return the `content` on success.
#[inline]
pub fn deserialize_message(buffer: Bytes) -> Result<Bytes, SchemaError> {
    let mut buffer = io::Cursor::new(buffer);

    let reader = serialize_packed::read_message(
        &mut buffer,
        ::capnp::message::ReaderOptions::new()
    )?;

    let message = reader.get_root::<channeler_capnp::message::Reader>()?;
    let content = Bytes::from(message.get_content()?);

    Ok(content)
}

/// Create and serialize a `EncryptMessage` with given `counter` and `content`(optional),
/// return the serialized message on success.
///
/// # Details
///
/// If content is `None`, the message type will be set to `KeepAlive`, whereas, it would be `User`.
#[inline]
pub fn serialize_enc_message(counter: u64, content: Option<Bytes>) -> Result<Bytes, SchemaError> {
    let mut builder = ::capnp::message::Builder::new_default();

    {
        let mut enc_message = builder.init_root::<encrypt_message::Builder>();

        enc_message.set_inc_counter(counter);

        if content.is_some() {
            enc_message.set_message_type(MessageType::User);
        } else {
            enc_message.set_message_type(MessageType::KeepAlive);
        }

        {
            let mut system_rng = OsRng::new()?;
            let padding_length = system_rng.next_u32() % 33;
            let rand_padding = enc_message.borrow().init_rand_padding(padding_length);
            system_rng.fill_bytes(&mut rand_padding[..padding_length as usize]);
        }

        if let Some(data) = content {
            let content = enc_message.borrow().init_content(data.len() as u32);
            // Optimize: Can we avoid copy here?
            content.copy_from_slice(data.as_ref());
        }
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;

    Ok(Bytes::from(serialized_msg))
}

/// Deserialize `EncryptMessage` from `buffer`, return the `incCounter`, `messageType` and
/// `content`(if not a keep alive message) on success.
#[inline]
pub fn deserialize_enc_message(buffer: Bytes)
    -> Result<(u64, MessageType, Option<Bytes>), SchemaError> {
    let mut buffer = io::Cursor::new(buffer);

    let reader = serialize_packed::read_message(
        &mut buffer,
        ::capnp::message::ReaderOptions::new()
    )?;

    let enc_message = reader.get_root::<encrypt_message::Reader>()?;

    // Read the incCounter
    let inc_counter = enc_message.get_inc_counter();

    // Read the messageType
    let message_type = enc_message.get_message_type()?;

    let content = match message_type {
        MessageType::KeepAlive => None,
        MessageType::User => Some(Bytes::from(enc_message.get_content()?))
    };

    Ok((inc_counter, message_type, content))
}

/// Create and serialize a `InitChannel` message from given `rand_value` and `public_key`,
/// return the serialized message on success.
#[inline]
pub fn serialize_init_channel_message(
    rand_value: RandValue,
    public_key: PublicKey
) -> Result<Bytes, SchemaError> {
    let mut builder = ::capnp::message::Builder::new_default();

    {
        let mut init_channel_msg = builder.init_root::<init_channel::Builder>();

        // Write the neighborPublicKey
        {
            let mut neighbor_public_key_writer = init_channel_msg.borrow().init_neighbor_public_key();
            write_public_key(&public_key, &mut neighbor_public_key_writer, )?;
        }
        // Write the channelRandValue
        {
            let mut channel_rand_value_writer = init_channel_msg.borrow().init_channel_rand_value();
            write_rand_value(&rand_value, &mut channel_rand_value_writer)?;
        }
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;

    Ok(Bytes::from(serialized_msg))
}

/// Deserialize `InitChannel` message from `buffer`, return the `neighborPublicKey` and
/// `channelRandValue` on success.
#[inline]
pub fn deserialize_init_channel_message(buffer: Bytes)
    -> Result<(PublicKey, RandValue), SchemaError> {
    let mut buffer = io::Cursor::new(buffer);

    let reader = serialize_packed::read_message(
        &mut buffer,
        ::capnp::message::ReaderOptions::new()
    )?;

    let init_channel_msg = reader.get_root::<init_channel::Reader>()?;

    // Read the neighborPublicKey
    let neighbor_public_key_reader = init_channel_msg.get_neighbor_public_key()?;
    let neighbor_public_key = read_public_key(&neighbor_public_key_reader)?;

    // Read the channelRandValue
    let channel_rand_value_reader = init_channel_msg.get_channel_rand_value()?;
    let channel_rand_value = read_rand_value(&channel_rand_value_reader)?;

    Ok((neighbor_public_key, channel_rand_value))
}

/// Create and serialize a `Exchange` message from given `dh_public_key`, `key_salt` and
/// `signature`, return the serialized message on success.
#[inline]
pub fn serialize_exchange_message(
    dh_public_key: DhPublicKey,
    key_salt: Salt,
    signature: Signature
) -> Result<Bytes, SchemaError> {
    let mut builder = ::capnp::message::Builder::new_default();

    {
        let mut exchange_msg = builder.init_root::<exchange::Builder>();

        // Write the commPublicKey
        {
            let mut dh_public_key_writer = exchange_msg.borrow().init_comm_public_key();
            write_dh_public_key(&dh_public_key, &mut dh_public_key_writer)?;
        }
        // Write the keySalt
        {
            let mut key_salt_writer = exchange_msg.borrow().init_key_salt();
            write_salt(&key_salt, &mut key_salt_writer)?;
        }
        // Write the signature
        {
            let mut signature_writer = exchange_msg.borrow().init_signature();
            write_signature(&signature, &mut signature_writer)?;
        }
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;

    Ok(Bytes::from(serialized_msg))
}

/// Deserialize `Exchange` message from `buffer`, return the `commPublicKey`, `keySalt` and
/// `signature` on success.
#[inline]
pub fn deserialize_exchange_message(buffer: Bytes)
    -> Result<(DhPublicKey, Salt, Signature), SchemaError> {
    let mut buffer = io::Cursor::new(buffer);

    let reader = serialize_packed::read_message(
        &mut buffer,
        ::capnp::message::ReaderOptions::new()
    )?;

    let exchange_msg = reader.get_root::<exchange::Reader>()?;

    // Read the commPublicKey
    let comm_public_key = read_dh_public_key(&exchange_msg.get_comm_public_key()?)?;

    // Read the keySalt
    let key_salt = read_salt(&exchange_msg.get_key_salt()?)?;

    // Read the signature
    let signature = read_signature(&exchange_msg.get_signature()?)?;

    Ok((comm_public_key, key_salt, signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_channel() {
        let in_public_key = PublicKey::from_bytes(&[0x03; 32]).unwrap();
        let in_rand_value = RandValue::from_bytes(&[0x06; 16]).unwrap();

        let serialized_init_channel = serialize_init_channel_message(
            in_rand_value.clone(),
            in_public_key.clone()
        ).unwrap();

        let (out_public_key, out_rand_value) =
            deserialize_init_channel_message(serialized_init_channel).unwrap();

        assert_eq!(in_public_key, out_public_key);
        assert_eq!(in_rand_value, out_rand_value);
    }

    #[test]
    fn test_exchange() {
        let in_comm_public_key = DhPublicKey::from_bytes(&[0x13; 32]).unwrap();
        let in_key_salt = Salt::from_bytes(&[0x16; 32]).unwrap();
        let in_signature = Signature::from_bytes(&[0x19; 64]).unwrap();

        let serialized_exchange = serialize_exchange_message(
            in_comm_public_key.clone(),
            in_key_salt.clone(),
            in_signature.clone()
        ).unwrap();

        let (out_comm_public_key, out_key_salt, out_signature) =
            deserialize_exchange_message(serialized_exchange).unwrap();

        assert_eq!(in_comm_public_key, out_comm_public_key);
        assert_eq!(in_key_salt, out_key_salt);
        assert_eq!(in_signature, out_signature);
    }

    #[test]
    fn test_enc_message() {
        let in_counter: u64 = 1 << 50;
        let in_content = Some(Bytes::from_static(&[0x11; 12345]));

        let serialized_enc_message = serialize_enc_message(
            in_counter,
            in_content.clone()
        ).unwrap();

        let (out_counter, out_message_type, out_content) =
            deserialize_enc_message(serialized_enc_message).unwrap();

        assert_eq!(in_counter, out_counter);
        assert_eq!(in_content, out_content);

        // capnpc not impl Debug for this
        assert!(MessageType::User == out_message_type);
    }

    #[test]
    fn test_message() {
        let in_content = Bytes::from_static(&[0x12; 3456]);

        let serialized_message = serialize_message(in_content.clone()).unwrap();

        let out_content = deserialize_message(serialized_message).unwrap();

        assert_eq!(in_content, out_content);
    }
}