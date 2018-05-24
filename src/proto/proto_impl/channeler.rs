use std::io;

use bytes::Bytes;
use capnp::serialize_packed;

use super::common::*;
use proto::channeler::*;
use proto::{Proto, ProtoError};

include_schema!(channeler_capnp, "channeler_capnp");

impl<'a> Proto<'a> for RequestNonce {
    type Reader = request_nonce::Reader<'a>;
    type Writer = request_nonce::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let request_rand_nonce =
            read_rand_value(&from.get_request_rand_nonce()?)?;

        Ok(RequestNonce { request_rand_nonce })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_rand_value(
            &self.request_rand_nonce,
            &mut to.reborrow().init_request_rand_nonce()
        )?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ResponseNonce {
    type Reader = response_nonce::Reader<'a>;
    type Writer = response_nonce::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let request_rand_nonce = read_rand_value(&from.get_request_rand_nonce()?)?;
        let response_rand_nonce = read_rand_value(&from.get_response_rand_nonce()?)?;
        let responder_rand_nonce = read_rand_value(&from.get_responder_rand_nonce()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ResponseNonce {
            request_rand_nonce,
            response_rand_nonce,
            responder_rand_nonce,
            signature
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_rand_value(
            &self.request_rand_nonce,
            &mut to.reborrow().init_request_rand_nonce()
        )?;

        write_rand_value(
            &self.response_rand_nonce,
            &mut to.reborrow().init_response_rand_nonce()
        )?;

        write_rand_value(
            &self.responder_rand_nonce,
            &mut to.reborrow().init_responder_rand_nonce()
        )?;

        write_signature(&self.signature, &mut to.reborrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ExchangeActive {
    type Reader = exchange_active::Reader<'a>;
    type Writer = exchange_active::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let responder_rand_nonce = read_rand_value(&from.get_responder_rand_nonce()?)?;
        let initiator_rand_nonce = read_rand_value(&from.get_initiator_rand_nonce()?)?;
        let initiator_public_key = read_public_key(&from.get_initiator_public_key()?)?;
        let responder_public_key = read_public_key(&from.get_responder_public_key()?)?;
        let dh_public_key = read_dh_public_key(&from.get_dh_public_key()?)?;
        let key_salt = read_salt(&from.get_key_salt()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ExchangeActive {
            responder_rand_nonce,
            initiator_rand_nonce,
            initiator_public_key,
            responder_public_key,
            dh_public_key,
            key_salt,
            signature,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_rand_value(&self.responder_rand_nonce, &mut to.reborrow().init_responder_rand_nonce())?;
        write_rand_value(&self.initiator_rand_nonce, &mut to.reborrow().init_initiator_rand_nonce())?;
        write_public_key(&self.initiator_public_key, &mut to.reborrow().init_initiator_public_key())?;
        write_public_key(&self.responder_public_key, &mut to.reborrow().init_responder_public_key())?;
        write_dh_public_key(&self.dh_public_key, &mut to.reborrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.reborrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.reborrow().init_signature())?;

        Ok(())
    }
}

impl<'a> Proto<'a> for ExchangePassive {
    type Reader = exchange_passive::Reader<'a>;
    type Writer = exchange_passive::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, ProtoError> {
        let prev_hash = read_hash_result(&from.get_prev_hash()?)?;
        let dh_public_key = read_dh_public_key(&from.get_dh_public_key()?)?;
        let key_salt = read_salt(&from.get_key_salt()?)?;
        let signature = read_signature(&from.get_signature()?)?;

        Ok(ExchangePassive { prev_hash, dh_public_key, key_salt, signature, })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        write_hash_result(&self.prev_hash, &mut to.reborrow().init_prev_hash())?;
        write_dh_public_key(&self.dh_public_key, &mut to.reborrow().init_dh_public_key())?;
        write_salt(&self.key_salt, &mut to.reborrow().init_key_salt())?;
        write_signature(&self.signature, &mut to.reborrow().init_signature())?;

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
        write_hash_result(&self.prev_hash, &mut to.reborrow().init_prev_hash())?;
        write_signature(&self.signature, &mut to.reborrow().init_signature())?;

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
        write_channel_id(&self.channel_id, &mut to.reborrow().init_channel_id())?;
        write_rand_value(&self.rand_nonce, &mut to.reborrow().init_rand_nonce())?;
        write_signature(&self.signature, &mut to.reborrow().init_signature())?;

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
            Which::Application(wrapped_content_reader) => {
                let content_reader = wrapped_content_reader?;
                Ok(PlainContent::Application(Bytes::from(content_reader)))
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        match *self {
            PlainContent::KeepAlive => to.set_keep_alive(()),
            PlainContent::Application(ref content) => {
                to.reborrow().init_application(content.len() as u32).copy_from_slice(content)
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
        to.reborrow().init_rand_padding(self.rand_padding.len() as u32)
            .copy_from_slice(&self.rand_padding);

        self.content.write(&mut to.reborrow().init_content())?;

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
            Which::RequestNonce(wrapped_request_nonce_reader) => {
                let request_nonce_reader = wrapped_request_nonce_reader?;

                Ok(ChannelerMessage::RequestNonce(
                    RequestNonce::read(&request_nonce_reader)?
                ))
            }
            Which::ResponseNonce(wrapped_response_nonce_reader) => {
                let respond_nonce_reader = wrapped_response_nonce_reader?;

                Ok(ChannelerMessage::ResponseNonce(
                    ResponseNonce::read(&respond_nonce_reader)?
                ))
            }
            Which::ExchangeActive(wrapped_exchange_active_reader) => {
                let exchange_active_reader = wrapped_exchange_active_reader?;

                Ok(ChannelerMessage::ExchangeActive(
                    ExchangeActive::read(&exchange_active_reader)?
                ))
            }
            Which::ExchangePassive(wrapped_exchange_passive_reader) => {
                let exchange_passive_reader = wrapped_exchange_passive_reader?;

                Ok(ChannelerMessage::ExchangePassive(
                    ExchangePassive::read(&exchange_passive_reader)?
                ))
            }
            Which::ChannelReady(wrapped_channel_ready_reader) => {
                let channel_ready_reader = wrapped_channel_ready_reader?;

                Ok(ChannelerMessage::ChannelReady(
                    ChannelReady::read(&channel_ready_reader)?
                ))
            }
            Which::UnknownChannel(wrapped_unknown_channel_reader) => {
                let unknown_channel_reader = wrapped_unknown_channel_reader?;

                Ok(ChannelerMessage::UnknownChannel(
                    UnknownChannel::read(&unknown_channel_reader)?
                ))
            }
            Which::Encrypted(wrapped_encrypted_content) => {
                let encrypted_content = Bytes::from(wrapped_encrypted_content?);

                Ok(ChannelerMessage::Encrypted(encrypted_content))
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), ProtoError> {
        match *self {
            ChannelerMessage::RequestNonce(ref request_nonce) => {
                request_nonce.write(&mut to.reborrow().init_request_nonce())?;
            }
            ChannelerMessage::ResponseNonce(ref response_nonce) => {
                response_nonce.write(&mut to.reborrow().init_response_nonce())?;
            }
            ChannelerMessage::ExchangeActive(ref exchange_active) => {
                exchange_active.write(&mut to.reborrow().init_exchange_active())?;
            }
            ChannelerMessage::ExchangePassive(ref exchange_passive) => {
                exchange_passive.write(&mut to.reborrow().init_exchange_passive())?;
            }
            ChannelerMessage::ChannelReady(ref channel_ready) => {
                channel_ready.write(&mut to.reborrow().init_channel_ready())?;
            }
            ChannelerMessage::UnknownChannel(ref unknown_channel) => {
                unknown_channel.write(&mut to.reborrow().init_unknown_channel())?;
            }
            ChannelerMessage::Encrypted(ref content) => {
                let dst = to.reborrow().init_encrypted(content.len() as u32);
                dst.copy_from_slice(content.as_ref());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::dh::{DhPublicKey, DH_PUBLIC_KEY_LEN, Salt, SALT_LEN};
    use crypto::hash::{HashResult, HASH_RESULT_LEN};
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN, Signature, SIGNATURE_LEN};
    use crypto::rand_values::{RandValue, RAND_VALUE_LEN};

    #[test]
    fn channeler_message_request_nonce() {
        let request_nonce = RequestNonce {
            request_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
        };

        let in_channeler_message = ChannelerMessage::RequestNonce(request_nonce);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_response_nonce() {
        let respond_nonce = ResponseNonce {
            request_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
            response_rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            responder_rand_nonce: RandValue::from(&[0x02; RAND_VALUE_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let in_channeler_message = ChannelerMessage::ResponseNonce(respond_nonce);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_exchange_active() {
        let exchange_active = ExchangeActive {
            responder_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
            initiator_rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            initiator_public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
            responder_public_key: PublicKey::from(&[0x03; PUBLIC_KEY_LEN]),
            dh_public_key: DhPublicKey::from(&[0x04; DH_PUBLIC_KEY_LEN]),
            key_salt: Salt::from(&[0x05; SALT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let in_channeler_message = ChannelerMessage::ExchangeActive(exchange_active);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_exchange_passive() {
        let exchange_passive = ExchangePassive {
            prev_hash: HashResult::from(&[0x00; HASH_RESULT_LEN]),
            dh_public_key: DhPublicKey::from(&[0x01; DH_PUBLIC_KEY_LEN]),
            key_salt: Salt::from(&[0x02; SALT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let in_channeler_message = ChannelerMessage::ExchangePassive(exchange_passive);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }

    #[test]
    fn channeler_message_channel_ready() {
        let channel_ready = ChannelReady {
            prev_hash: HashResult::from(&[0x00; HASH_RESULT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let in_channeler_message = ChannelerMessage::ChannelReady(channel_ready);

        test_encode_decode!(ChannelerMessage, in_channeler_message);
    }


    #[test]
    fn channeler_message_unknown_channel() {
        let unknown_channel = UnknownChannel {
            channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
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
    fn channeler_message_encrypted_application() {
        let plain_application = Plain {
            rand_padding: Bytes::from("rand_padding"),
            content: PlainContent::Application(Bytes::from("application data")),
        };

        let serialized_plain_application = plain_application.encode().unwrap();

        let channeler_message_encrypted_application =
            ChannelerMessage::Encrypted(serialized_plain_application);

        test_encode_decode!(ChannelerMessage, channeler_message_encrypted_application);
    }
}
