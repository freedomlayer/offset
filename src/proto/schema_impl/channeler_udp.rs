use std::io;

use bytes::Bytes;
use capnp::serialize_packed;

use super::common::*;
use proto::channeler_udp::*;
use proto::{Schema, SchemaError};

include_schema!(channeler_udp_capnp, "channeler_udp_capnp");


impl<'a> Schema<'a> for InitChannel {
    type Reader = init_channel::Reader<'a>;
    type Writer = init_channel::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let rand_nonce = read_rand_value(&from.get_rand_nonce()?)?;
        let public_key = read_public_key(&from.get_public_key()?)?;

        Ok(InitChannel { rand_nonce, public_key })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_public_key(&self.public_key, &mut to.borrow().init_public_key())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for ExchangePassive {
    type Reader = exchange_passive::Reader<'a>;
    type Writer = exchange_passive::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
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

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_public_key(&self.public_key, &mut to.borrow().init_public_key())?;
        write_dh_public_key(&self.dh_public_key, &mut to.borrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.borrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for ExchangeActive {
    type Reader = exchange_active::Reader<'a>;
    type Writer = exchange_active::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
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

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_dh_public_key(&self.dh_public_key, &mut to.borrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.borrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for ChannelReady {
    type Reader = channel_ready::Reader<'a>;
    type Writer = channel_ready::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let prev_hash = read_hash_result(&from.get_prev_hash()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ChannelReady {
            prev_hash,
            signature
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_hash_result(&self.prev_hash, &mut to.borrow().init_prev_hash())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for UnknownChannel {
    type Reader = unknown_channel::Reader<'a>;
    type Writer = unknown_channel::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let channel_id = read_channel_id(&from.get_channel_id()?)?;
        let rand_nonce = read_rand_value(&from.get_rand_nonce()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(UnknownChannel {
            channel_id,
            rand_nonce,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_channel_id(&self.channel_id, &mut to.borrow().init_channel_id())?;
        write_rand_value(&self.rand_nonce, &mut to.borrow().init_rand_nonce())?;
        write_signature(&self.signature, &mut to.borrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for PlainContent {
    type Reader = plain::content::Reader<'a>;
    type Writer = plain::content::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        use self::plain::content::Which;

        match from.which()? {
            Which::KeepAlive(()) => Ok(PlainContent::KeepAlive),
            Which::User(wrapped_content_reader) => {
                let content_reader = wrapped_content_reader?;
                Ok(PlainContent::User(Bytes::from(content_reader)))
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        match *self {
            PlainContent::KeepAlive => to.set_keep_alive(()),
            PlainContent::User(ref content) => {
                to.borrow().init_user(content.len() as u32)
                    .copy_from_slice(&content)
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for Plain {
    type Reader = plain::Reader<'a>;
    type Writer = plain::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let rand_padding = Bytes::from(from.get_rand_padding()?);
        let content = PlainContent::read(&from.get_content())?;

        Ok(Plain {
            rand_padding,
            content
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        to.borrow().init_rand_padding(self.rand_padding.len() as u32)
            .copy_from_slice(&self.rand_padding);

        self.content.write(&mut to.borrow().init_content())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for ChannelerMessage {
    type Reader = channeler_message::Reader<'a>;
    type Writer = channeler_message::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
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

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
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
                let mut dst = to.borrow().init_encrypted(content.len() as u32);
                dst.copy_from_slice(content.as_ref());
            }
        }

        Ok(())
    }
}

