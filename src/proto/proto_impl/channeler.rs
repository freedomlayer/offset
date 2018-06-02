use std::io;

use bytes::Bytes;
use capnp::serialize_packed;

use super::common::*;
use proto::channeler::*;
use proto::{Proto, ProtoError};

include_schema!(channeler_capnp, "channeler_capnp");

impl<'a> Proto<'a> for InitChannel {
    type Reader = init_channel::Reader<'a>;
    type Writer = init_channel::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let rand_nonce = read_rand_value(&from.get_rand_nonce()?)?;
        let public_key = read_public_key(&from.get_public_key()?)?;

        Ok(InitChannel { rand_nonce, public_key })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_public_key(&self.public_key, &mut to.borrow().init_public_key())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ExchangePassive {
    type Reader = exchange_passive::Reader<'a>;
    type Writer = exchange_passive::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let prev_hash = read_hash_result(&from.get_prev_hash()?)?;
        let rand_nonce = read_rand_value(&from.get_rand_nonce()?)?;
        let public_key = read_public_key(&from.get_public_key()?)?;
        let dh_public_key = read_dh_public_key(&from.get_dh_public_key()?)?;
        let key_salt = read_salt(&from.get_key_salt()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ExchangePassive {
            prev_hash,
            rand_nonce,
            public_key,
            dh_public_key,
            key_salt,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_public_key(&self.public_key, &mut to.borrow().init_public_key())?;
        write_dh_public_key(&self.dh_public_key, &mut to.borrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.borrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ExchangeActive {
    type Reader = exchange_active::Reader<'a>;
    type Writer = exchange_active::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let prev_hash = read_hash_result(&from.get_prev_hash()?)?;
        let dh_public_key = read_dh_public_key(&from.get_dh_public_key()?)?;
        let key_salt = read_salt(&from.get_key_salt()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ExchangeActive {
            prev_hash,
            dh_public_key,
            key_salt,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_dh_public_key(&self.dh_public_key, &mut to.borrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.borrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ChannelReady {
    type Reader = channel_ready::Reader<'a>;
    type Writer = channel_ready::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let prev_hash = read_hash_result(&from.get_prev_hash()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ChannelReady {
            prev_hash,
            signature
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for UnknownChannel {
    type Reader = unknown_channel::Reader<'a>;
    type Writer = unknown_channel::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let channel_id = read_channel_id(&from.get_channel_id()?)?;
        let rand_nonce = read_rand_value(&from.get_rand_nonce()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(UnknownChannel {
            channel_id,
            rand_nonce,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_channel_id(&self.channel_id, &mut to.borrow().init_channel_id())?;
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for PlainContent {
    type Reader = plain::content::Reader<'a>;
    type Writer = plain::content::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        use self::plain::content::Which;

        match from.which()? {
            Which::KeepAlive(()) => Ok(PlainContent::KeepAlive),
            Which::User(wrapped_content_reader) => {
                let content_reader = wrapped_content_reader?;
                Ok(PlainContent::User(Bytes::from(content_reader)))
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        match *self {
            PlainContent::KeepAlive => to.set_keep_alive(()),
            PlainContent::User(ref content) => {
                to.borrow().init_user(content.len() as u32).copy_from_slice(content)
            }
        }

        Ok(())
    }
}

impl<'a> Proto<'a> for Plain {
    type Reader = plain::Reader<'a>;
    type Writer = plain::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let rand_padding = Bytes::from(from.get_rand_padding()?);
        let content = PlainContent::read(&from.get_content())?;

        Ok(Plain {
            rand_padding,
            content
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        to.borrow().init_rand_padding(self.rand_padding.len() as u32)
            .copy_from_slice(&self.rand_padding);

        self.content.write(&mut to.borrow().init_content())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ChannelerMessage {
    type Reader = channeler_message::Reader<'a>;
    type Writer = channeler_message::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        use self::channeler_message::Which;

        match from.which()? {
            Which::InitChannel(wrapped_init_channel_reader) => {
                let init_channel_reader = wrapped_init_channel_reader?;

                Ok(ChannelerMessage::InitChannel(
                    InitChannel::read(&init_channel_reader)?
                ))
            }
            Which::ExchangePassive(wrapped_exchange_passive) => {
                let exchange_passive = wrapped_exchange_passive?;

                Ok(ChannelerMessage::ExchangePassive(
                    ExchangePassive::read(&exchange_passive)?
                ))
            }
            Which::ExchangeActive(wrapped_exchange_active) => {
                let exchange_active = wrapped_exchange_active?;

                Ok(ChannelerMessage::ExchangeActive(
                    ExchangeActive::read(&exchange_active)?
                ))
            }
            Which::ChannelReady(wrapped_channel_ready) => {
                let channel_ready_reader = wrapped_channel_ready?;

                Ok(ChannelerMessage::ChannelReady(
                    ChannelReady::read(&channel_ready_reader)?
                ))
            }
            Which::UnknownChannel(wrapped_unknown_channel) => {
                let unknown_channel_reader = wrapped_unknown_channel?;

                Ok(ChannelerMessage::UnknownChannel(
                    UnknownChannel::read(&unknown_channel_reader)?
                ))
            }
            Which::Encrypted(wrapped_encrypted) => {
                let content = Bytes::from(wrapped_encrypted?);

                Ok(ChannelerMessage::Encrypted(content))
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        match *self {
            ChannelerMessage::InitChannel(ref init_channel) => {
                init_channel.write(&mut to.borrow().init_init_channel())?;
            }
            ChannelerMessage::ExchangePassive(ref exchange_passive) => {
                exchange_passive.write(&mut to.borrow().init_exchange_passive())?;
            }
            ChannelerMessage::ExchangeActive(ref exchange_active) => {
                exchange_active.write(&mut to.borrow().init_exchange_active())?;
            }
            ChannelerMessage::ChannelReady(ref channel_ready) => {
                channel_ready.write(&mut to.borrow().init_channel_ready())?;
            }
            ChannelerMessage::UnknownChannel(ref unknown_channel) => {
                unknown_channel.write(&mut to.borrow().init_unknown_channel())?;
            }
            ChannelerMessage::Encrypted(ref content) => {
                let dst = to.borrow().init_encrypted(content.len() as u32);
                dst.copy_from_slice(content.as_ref());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    use crypto::dh::{DhPublicKey, DH_PUBLIC_KEY_LEN, Salt, SALT_LEN};
    use crypto::hash::{HashResult, HASH_RESULT_LEN};
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN, Signature, SIGNATURE_LEN};
    use crypto::rand_values::{RandValue, RAND_VALUE_LEN};

    #[test]
    fn channeler_message_init_channel() {
        let init_channel = InitChannel {
            rand_nonce: RandValue::try_from(&[0x7f; RAND_VALUE_LEN]).unwrap(),
            public_key: PublicKey::try_from(&[0xf7; PUBLIC_KEY_LEN]).unwrap(),
        };

        let in_channeler_message = ChannelerMessage::InitChannel(init_channel);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_exchange_passive() {
        let exchange_passive = ExchangePassive {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN][..]).unwrap(),
            rand_nonce: RandValue::try_from(&[0x02; RAND_VALUE_LEN]).unwrap(),
            public_key: PublicKey::try_from(&[0x03; PUBLIC_KEY_LEN]).unwrap(),
            dh_public_key: DhPublicKey::try_from(&[0x04; DH_PUBLIC_KEY_LEN]).unwrap(),
            key_salt: Salt::try_from(&[0x05; SALT_LEN]).unwrap(),
            signature: Signature::try_from(&[0x06; SIGNATURE_LEN]).unwrap(),
        };

        let in_channeler_message = ChannelerMessage::ExchangePassive(exchange_passive);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }


    #[test]
    fn channeler_message_exchange_active() {
        let exchange_active = ExchangeActive {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN]).unwrap(),
            dh_public_key: DhPublicKey::try_from(&[0x02; DH_PUBLIC_KEY_LEN]).unwrap(),
            key_salt: Salt::try_from(&[0x03; SALT_LEN]).unwrap(),
            signature: Signature::try_from(&[0x04; SIGNATURE_LEN]).unwrap(),
        };

        let in_channeler_message = ChannelerMessage::ExchangeActive(exchange_active);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }


    #[test]
    fn channeler_message_channel_ready() {
        let channel_ready = ChannelReady {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN][..]).unwrap(),
            signature: Signature::try_from(&[0x02; SIGNATURE_LEN]).unwrap(),
        };

        let in_channeler_message = ChannelerMessage::ChannelReady(channel_ready);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }


    #[test]
    fn channeler_message_unknown_channel() {
        let unknown_channel = UnknownChannel {
            channel_id: ChannelId::try_from(&[0x01u8; CHANNEL_ID_LEN][..]).unwrap(),
            rand_nonce: RandValue::try_from(&[0x02; RAND_VALUE_LEN]).unwrap(),
            signature: Signature::try_from(&[0x03; SIGNATURE_LEN]).unwrap(),
        };

        let in_channeler_message = ChannelerMessage::UnknownChannel(unknown_channel);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_encrypted_keepalive() {
        let plain_keepalive = Plain {
            rand_padding: Bytes::from("rand_padding"),
            content: PlainContent::KeepAlive,
        };

        let serialized_plain_keepalive = plain_keepalive.encode().unwrap();

        let channeler_message_encrypted_keepalive =
            ChannelerMessage::Encrypted(serialized_plain_keepalive);

        test_encode_decode!(ChannelerMessage, channeler_message_encrypted_keepalive);
    }

    #[test]
    fn channeler_message_encrypted_user() {
        let plain_user = Plain {
            rand_padding: Bytes::from("rand_padding"),
            content: PlainContent::User(Bytes::from("user data")),
        };

        let serialized_plain_user = plain_user.encode().unwrap();

        let channeler_message_encrypted_user =
            ChannelerMessage::Encrypted(serialized_plain_user);

        test_encode_decode!(ChannelerMessage, channeler_message_encrypted_user);
    }
}
