#![allow(unused)]
use std::io;
use std::convert::TryFrom;
use capnp;
use capnp::serialize_packed;
use dh_capnp;
use crypto::identity::PublicKey;
use utils::capnp_custom_int::{read_custom_u_int128, write_custom_u_int128,
                                read_custom_u_int256, write_custom_u_int256,
                                read_custom_u_int512, write_custom_u_int512};
// use dh_capnp::{plain, exchange_rand_nonce, exchange_dh, rekey};
use super::messages::{ChannelMessage, ExchangeRandNonce, ExchangeDh, Rekey};

pub enum DhSerializeError {
    CapnpError(capnp::Error),
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

fn serialize_rekey(rekey: &Rekey) -> Result<Vec<u8>, DhSerializeError> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<dh_capnp::rekey::Builder>();

    write_custom_u_int256(&rekey.dh_public_key, &mut msg.reborrow().get_dh_public_key()?);
    write_custom_u_int256(&rekey.key_salt, &mut msg.reborrow().get_key_salt()?);

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder)?;
    Ok(serialized_msg)
}


