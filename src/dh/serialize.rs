#![allow(unused)]
use std::io;
use std::convert::TryFrom;
use capnp;
use capnp::serialize_packed;
use dh_capnp;
use crypto::identity::{PublicKey, Signature};
use crypto::rand_values::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use utils::capnp_custom_int::{read_custom_u_int128, write_custom_u_int128,
                                read_custom_u_int256, write_custom_u_int256,
                                read_custom_u_int512, write_custom_u_int512};
// use dh_capnp::{plain, exchange_rand_nonce, exchange_dh, rekey};
use super::messages::{PlainData, ChannelMessage, ChannelContent, 
    ExchangeRandNonce, ExchangeDh, Rekey};

pub enum DhSerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}


impl From<capnp::Error> for DhSerializeError {
    fn from(e: capnp::Error) -> DhSerializeError {
        DhSerializeError::CapnpError(e)
    }
}

impl From<io::Error> for DhSerializeError {
    fn from(e: io::Error) -> DhSerializeError {
        DhSerializeError::IoError(e)
    }
}

fn serialize_exchange_rand_nonce(exchange_rand_nonce: &ExchangeRandNonce) -> Result<Vec<u8>, DhSerializeError> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::exchange_rand_nonce::Builder>();

    write_custom_u_int128(&exchange_rand_nonce.rand_nonce, &mut msg.reborrow().get_rand_nonce()?);
    write_custom_u_int256(&exchange_rand_nonce.public_key, &mut msg.reborrow().get_public_key()?);

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;
    Ok(serialized_msg)
}

fn deserialize_exchange_rand_nonce(data: Vec<u8>) -> Result<ExchangeRandNonce, DhSerializeError> {
    let mut cursor = io::Cursor::new(&data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::exchange_rand_nonce::Reader>()?;

    let rand_nonce = RandValue::try_from(&read_custom_u_int128(&msg.get_rand_nonce()?)[..]).unwrap();
    let public_key = PublicKey::try_from(&read_custom_u_int256(&msg.get_public_key()?)[..]).unwrap();

    Ok(ExchangeRandNonce {
        rand_nonce,
        public_key,
    })
}


fn serialize_exchange_dh(exchange_dh: &ExchangeDh) -> Result<Vec<u8>, DhSerializeError> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::exchange_dh::Builder>();

    write_custom_u_int256(&exchange_dh.dh_public_key, &mut msg.reborrow().get_dh_public_key()?);
    write_custom_u_int128(&exchange_dh.rand_nonce, &mut msg.reborrow().get_rand_nonce()?);
    write_custom_u_int256(&exchange_dh.key_salt, &mut msg.reborrow().get_key_salt()?);
    write_custom_u_int512(&exchange_dh.signature, &mut msg.reborrow().get_signature()?);

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;
    Ok(serialized_msg)
}

fn deserialize_exchange_dh(data: Vec<u8>) -> Result<ExchangeDh, DhSerializeError> {
    let mut cursor = io::Cursor::new(&data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::exchange_dh::Reader>()?;

    let dh_public_key = DhPublicKey::try_from(&read_custom_u_int256(&msg.get_dh_public_key()?)[..]).unwrap();
    let rand_nonce = RandValue::try_from(&read_custom_u_int128(&msg.get_rand_nonce()?)[..]).unwrap();
    let key_salt = Salt::try_from(&read_custom_u_int256(&msg.get_key_salt()?)[..]).unwrap();
    let signature = Signature::try_from(&read_custom_u_int512(&msg.get_signature()?)[..]).unwrap();

    Ok(ExchangeDh {
        dh_public_key,
        rand_nonce,
        key_salt,
        signature,
    })
}

/*
fn serialize_rekey(rekey: &Rekey) -> Result<Vec<u8>, DhSerializeError> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::rekey::Builder>();

    write_custom_u_int256(&rekey.dh_public_key, &mut msg.reborrow().get_dh_public_key()?);
    write_custom_u_int256(&rekey.key_salt, &mut msg.reborrow().get_key_salt()?);

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;
    Ok(serialized_msg)
}
*/

fn serialize_channel_message(channel_message: &ChannelMessage) -> Result<Vec<u8>, DhSerializeError> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::channel_message::Builder>();
    let mut serialized_msg = Vec::new();

    msg.reborrow().set_rand_padding(&channel_message.rand_padding);
    let mut content_msg = msg.reborrow().get_content();

    match &channel_message.content {
        ChannelContent::KeepAlive => {
            content_msg.set_keep_alive(());
        }, 
        ChannelContent::Rekey(rekey) => {
            let mut rekey_msg = content_msg.init_rekey();
            write_custom_u_int256(&rekey.dh_public_key, &mut rekey_msg.reborrow().get_dh_public_key()?);
            write_custom_u_int256(&rekey.key_salt, &mut rekey_msg.reborrow().get_key_salt()?);
        },
        ChannelContent::User(PlainData(plain_data)) => {
            content_msg.set_user(plain_data);
        },
    };

    serialize_packed::write_message(&mut serialized_msg, &builder)?;
    Ok(serialized_msg)
}


fn deserialize_channel_message(data: Vec<u8>) -> Result<ChannelMessage, DhSerializeError> {
    let mut cursor = io::Cursor::new(&data);
    let reader = serialize_packed::read_message(&mut cursor, ::capnp::message::ReaderOptions::new())?;
    let msg = reader.get_root::<dh_capnp::channel_message::Reader>()?;

    let rand_padding = msg.get_rand_padding()?.to_vec();
    let content = match msg.get_content().which() {
        Ok(dh_capnp::channel_message::content::KeepAlive(())) => ChannelContent::KeepAlive,
        Ok(dh_capnp::channel_message::content::Rekey(rekey)) => {
            let rekey = rekey?;
            let dh_public_key = DhPublicKey::try_from(&read_custom_u_int256(&rekey.get_dh_public_key()?)[..]).unwrap();
            let key_salt = Salt::try_from(&read_custom_u_int256(&rekey.get_key_salt()?)[..]).unwrap();
            ChannelContent::Rekey(Rekey {
                dh_public_key,
                key_salt,
            })
        },
        Ok(dh_capnp::channel_message::content::User(data)) => ChannelContent::User(PlainData(data?.to_vec())),
        Err(e) => return Err(DhSerializeError::NotInSchema(e)),
    };

    Ok(ChannelMessage {
        rand_padding,
        content,
    })
}
