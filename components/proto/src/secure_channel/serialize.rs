use std::io;
use capnp;
use capnp::serialize_packed;
use dh_capnp;

use crate::capnp_common::{write_public_key, read_public_key,
                        write_rand_nonce, read_rand_nonce,
                        read_dh_public_key, write_dh_public_key,
                        write_salt, read_salt,
                        write_signature, read_signature};

use crate::serialize::SerializeError;

use super::messages::{PlainData, ChannelMessage, ChannelContent, 
    ExchangeRandNonce, ExchangeDh, Rekey};


pub fn serialize_exchange_rand_nonce(exchange_rand_nonce: &ExchangeRandNonce) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::exchange_rand_nonce::Builder>();

    write_rand_nonce(&exchange_rand_nonce.rand_nonce, &mut msg.reborrow().get_rand_nonce().unwrap());
    write_public_key(&exchange_rand_nonce.public_key, &mut msg.reborrow().get_public_key().unwrap());

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_exchange_rand_nonce(data: &[u8]) -> Result<ExchangeRandNonce, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::exchange_rand_nonce::Reader>()?;

    let rand_nonce = read_rand_nonce(&msg.get_rand_nonce()?)?;
    let public_key = read_public_key(&msg.get_public_key()?)?;

    Ok(ExchangeRandNonce {
        rand_nonce,
        public_key,
    })
}


pub fn serialize_exchange_dh(exchange_dh: &ExchangeDh) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::exchange_dh::Builder>();

    write_dh_public_key(&exchange_dh.dh_public_key, &mut msg.reborrow().get_dh_public_key().unwrap());
    write_rand_nonce(&exchange_dh.rand_nonce, &mut msg.reborrow().get_rand_nonce().unwrap());
    write_salt(&exchange_dh.key_salt, &mut msg.reborrow().get_key_salt().unwrap());
    write_signature(&exchange_dh.signature, &mut msg.reborrow().get_signature().unwrap());

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}

pub fn deserialize_exchange_dh(data: &[u8]) -> Result<ExchangeDh, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::exchange_dh::Reader>()?;

    let dh_public_key = read_dh_public_key(&msg.get_dh_public_key()?)?;
    let rand_nonce = read_rand_nonce(&msg.get_rand_nonce()?)?;
    let key_salt = read_salt(&msg.get_key_salt()?)?;
    let signature = read_signature(&msg.get_signature()?)?;

    Ok(ExchangeDh {
        dh_public_key,
        rand_nonce,
        key_salt,
        signature,
    })
}

pub fn serialize_channel_message(channel_message: &ChannelMessage) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::channel_message::Builder>();
    let mut serialized_msg = Vec::new();

    msg.reborrow().set_rand_padding(&channel_message.rand_padding);
    let mut content_msg = msg.reborrow().get_content();

    match &channel_message.content {
        ChannelContent::Rekey(rekey) => {
            let mut rekey_msg = content_msg.init_rekey();
            write_dh_public_key(&rekey.dh_public_key, &mut rekey_msg.reborrow().get_dh_public_key().unwrap());
            write_salt(&rekey.key_salt, &mut rekey_msg.reborrow().get_key_salt().unwrap());
        },
        ChannelContent::User(PlainData(plain_data)) => {
            content_msg.set_user(plain_data);
        },
    };

    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
}


pub fn deserialize_channel_message(data: &[u8]) -> Result<ChannelMessage, SerializeError> {
    let mut cursor = io::Cursor::new(data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::channel_message::Reader>()?;

    let rand_padding = msg.get_rand_padding()?.to_vec();
    let content = match msg.get_content().which() {
        Ok(dh_capnp::channel_message::content::Rekey(rekey)) => {
            let rekey = rekey?;
            let dh_public_key = read_dh_public_key(&rekey.get_dh_public_key()?)?;
            let key_salt = read_salt(&rekey.get_key_salt()?)?;
            ChannelContent::Rekey(Rekey {
                dh_public_key,
                key_salt,
            })
        },
        Ok(dh_capnp::channel_message::content::User(data)) => ChannelContent::User(PlainData(data?.to_vec())),
        Err(e) => return Err(SerializeError::NotInSchema(e)),
    };

    Ok(ChannelMessage {
        rand_padding,
        content,
    })
}



#[cfg(test)]
mod tests {
    use super::*;
    use crypto::crypto_rand::RAND_VALUE_LEN;
    use crypto::identity::{PUBLIC_KEY_LEN, SIGNATURE_LEN};
    use crypto::dh::{SALT_LEN, DH_PUBLIC_KEY_LEN};
    use std::convert::TryFrom;
    use crypto::identity::{PublicKey, Signature};
    use crypto::crypto_rand::RandValue;
    use crypto::dh::{DhPublicKey, Salt};

    #[test]
    fn test_serialize_exchange_rand_nonce() {
        let msg = ExchangeRandNonce {
            rand_nonce: RandValue::try_from(&[0x01u8; RAND_VALUE_LEN][..]).unwrap(),
            public_key: PublicKey::try_from(&[0x02u8; PUBLIC_KEY_LEN][..]).unwrap(),
        };
        let serialized = serialize_exchange_rand_nonce(&msg);
        let msg2 = deserialize_exchange_rand_nonce(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_serialize_exchange_dh() {
        let msg = ExchangeDh {
            dh_public_key: DhPublicKey::try_from(&[0x01u8; DH_PUBLIC_KEY_LEN][..]).unwrap(),
            rand_nonce: RandValue::try_from(&[0x02u8; RAND_VALUE_LEN][..]).unwrap(),
            key_salt: Salt::try_from(&[0x03u8; SALT_LEN][..]).unwrap(),
            signature: Signature::try_from(&[0x03u8; SIGNATURE_LEN][..]).unwrap(),
        };
        let serialized = serialize_exchange_dh(&msg);
        let msg2 = deserialize_exchange_dh(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }

    #[test]
    fn test_serialize_channel_message_rekey() {
        let rekey = Rekey {
            dh_public_key: DhPublicKey::try_from(&[0x01u8; DH_PUBLIC_KEY_LEN][..]).unwrap(),
            key_salt: Salt::try_from(&[0x03u8; SALT_LEN][..]).unwrap(),
        };
        let content = ChannelContent::Rekey(rekey);
        let msg = ChannelMessage {
            rand_padding: vec![1,2,3,4,5,6],
            content,
        };
        let serialized = serialize_channel_message(&msg);
        let msg2 = deserialize_channel_message(&serialized[..]).unwrap();
        assert_eq!(msg, msg2);
    }
}
