use std::io;

use bytes::Bytes;
use capnp::serialize_packed;

include_schema!(channeler_capnp, "channeler_capnp");

// Re-export the `MessageType`
pub use self::channeler_capnp::MessageType;

use proto::{Proto, ProtoError};
use proto::channeler::{EncryptMessage, Exchange, InitChannelActive, InitChannelPassive};

use super::common::{read_dh_public_key, read_public_key, read_rand_value, read_salt,
                    read_signature, write_dh_public_key, write_public_key, write_rand_value,
                    write_salt, write_signature};

/// Create and serialize a `Message` from given `content`,
/// return the serialized message on success.
#[inline]
pub fn serialize_message(content: Bytes) -> Result<Bytes, ProtoError> {
    let mut builder = ::capnp::message::Builder::new_default();

    {
        let mut msg = builder.init_root::<message::Builder>();
        let msg_content = msg.borrow().init_content(content.len() as u32);
        // Optimize: Can we avoid copy here?
        msg_content.copy_from_slice(content.as_ref());
    }

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;

    Ok(Bytes::from(serialized_msg))
}

/// Deserialize `Message` from `buffer`, return the `content` on success.
#[inline]
pub fn deserialize_message(buffer: Bytes) -> Result<Bytes, ProtoError> {
    let mut buffer = io::Cursor::new(buffer);

    let reader =
        serialize_packed::read_message(&mut buffer, ::capnp::message::ReaderOptions::new())?;

    let msg = reader.get_root::<message::Reader>()?;
    let content = Bytes::from(msg.get_content()?);

    Ok(content)
}

impl<'a> Proto<'a> for InitChannelActive {
    type Reader = init_channel_active::Reader<'a>;
    type Writer = init_channel_active::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let neighbor_public_key = read_public_key(&from.get_neighbor_public_key()?)?;

        let channel_rand_value = read_rand_value(&from.get_channel_rand_value()?)?;

        let channel_index = from.get_channel_index();

        Ok(InitChannelActive {
            neighbor_public_key,
            channel_rand_value,
            channel_index,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_public_key(
            &self.neighbor_public_key,
            &mut to.borrow().init_neighbor_public_key(),
        )?;
        write_rand_value(
            &self.channel_rand_value,
            &mut to.borrow().init_channel_rand_value(),
        )?;

        to.borrow().set_channel_index(self.channel_index);

        Ok(())
    }
}

impl<'a> Proto<'a> for InitChannelPassive {
    type Reader = init_channel_passive::Reader<'a>;
    type Writer = init_channel_passive::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let neighbor_public_key = read_public_key(&from.get_neighbor_public_key()?)?;

        let channel_rand_value = read_rand_value(&from.get_channel_rand_value()?)?;

        Ok(InitChannelPassive {
            neighbor_public_key,
            channel_rand_value,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_public_key(
            &self.neighbor_public_key,
            &mut to.borrow().init_neighbor_public_key(),
        )?;
        write_rand_value(
            &self.channel_rand_value,
            &mut to.borrow().init_channel_rand_value(),
        )?;

        Ok(())
    }
}

impl<'a> Proto<'a> for Exchange {
    type Reader = exchange::Reader<'a>;
    type Writer = exchange::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let comm_public_key = read_dh_public_key(&from.get_comm_public_key()?)?;

        let key_salt = read_salt(&from.get_key_salt()?)?;

        let signature = read_signature(&from.get_signature()?)?;

        Ok(Exchange {
            comm_public_key,
            key_salt,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_dh_public_key(
            &self.comm_public_key,
            &mut to.borrow().init_comm_public_key(),
        )?;
        write_salt(&self.key_salt, &mut to.borrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for EncryptMessage {
    type Reader = encrypt_message::Reader<'a>;
    type Writer = encrypt_message::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let inc_counter = from.get_inc_counter();
        let rand_padding = Bytes::from(from.get_rand_padding()?);
        let message_type = from.get_message_type()?;
        let content = Bytes::from(from.get_content()?);

        Ok(EncryptMessage {
            inc_counter,
            rand_padding,
            message_type,
            content,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        to.set_inc_counter(self.inc_counter);
        to.set_message_type(self.message_type);

        to.borrow()
            .init_rand_padding(self.rand_padding.len() as u32)
            .copy_from_slice(&self.rand_padding);

        to.borrow()
            .init_content(self.content.len() as u32)
            .copy_from_slice(&self.content);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    use crypto::dh::{DhPublicKey, Salt};
    use crypto::identity::{PublicKey, Signature};
    use crypto::rand_values::RandValue;

    #[test]
    fn init_channel_active() {
        let neighbor_public_key = PublicKey::try_from(&[0x03; 32]).unwrap();
        let channel_rand_value = RandValue::try_from(&[0x06; 16]).unwrap();

        let in_init_channel_active = InitChannelActive {
            neighbor_public_key,
            channel_rand_value,
            channel_index: 64,
        };

        test_encode_decode!(InitChannelActive, in_init_channel_active);
    }

    #[test]
    fn init_channel_passive() {
        let neighbor_public_key = PublicKey::try_from(&[0x03; 32]).unwrap();
        let channel_rand_value = RandValue::try_from(&[0x06; 16]).unwrap();

        let in_init_channel_passive = InitChannelPassive {
            neighbor_public_key,
            channel_rand_value,
        };

        test_encode_decode!(InitChannelPassive, in_init_channel_passive);
    }

    #[test]
    fn exchange() {
        let comm_public_key = DhPublicKey::try_from(&[0x13; 32]).unwrap();
        let key_salt = Salt::try_from(&[0x16; 32]).unwrap();
        let signature = Signature::try_from(&[0x19; 64]).unwrap();

        let in_exchange = Exchange {
            comm_public_key,
            key_salt,
            signature,
        };

        test_encode_decode!(Exchange, in_exchange);
    }

    #[test]
    fn encrypt_message() {
        let inc_counter: u64 = 1 << 50;
        let rand_padding = Bytes::from_static(&[0x23; 32]);
        let message_type = MessageType::User;
        let content = Bytes::from_static(&[0x11; 12345]);

        let in_encrypt_message = EncryptMessage {
            inc_counter,
            message_type,
            rand_padding,
            content,
        };

        test_encode_decode!(EncryptMessage, in_encrypt_message);
    }

    #[test]
    fn message() {
        let in_content = Bytes::from_static(&[0x12; 3456]);

        let serialized_message = serialize_message(in_content.clone()).unwrap();

        let out_content = deserialize_message(serialized_message).unwrap();

        assert_eq!(in_content, out_content);
    }
}
