#![allow(unused)]
use std::io;
use std::convert::TryFrom;
use capnp;
use capnp::serialize_packed;

use relay_capnp;

use super::messages::{TunnelMessage};

#[derive(Debug)]
pub enum RelaySerializeError {
    CapnpError(capnp::Error),
    NotInSchema(capnp::NotInSchema),
    IoError(io::Error),
}


impl From<capnp::Error> for RelaySerializeError {
    fn from(e: capnp::Error) -> RelaySerializeError {
        RelaySerializeError::CapnpError(e)
    }
}

impl From<io::Error> for RelaySerializeError {
    fn from(e: io::Error) -> RelaySerializeError {
        RelaySerializeError::IoError(e)
    }
}

pub fn serialize_tunnel_message(tunnel_message: &TunnelMessage) -> Vec<u8> {
    let mut builder = capnp::message::Builder::new_default();
    let mut msg = builder.init_root::<relay_capnp::tunnel_message::Builder>();

    /*
    write_custom_u_int128(&exchange_rand_nonce.rand_nonce, &mut msg.reborrow().get_rand_nonce().unwrap());
    write_custom_u_int256(&exchange_rand_nonce.public_key, &mut msg.reborrow().get_public_key().unwrap());

    let mut serialized_msg = Vec::new();
    serialize_packed::write_message(&mut serialized_msg, &builder).unwrap();
    serialized_msg
    */
    unimplemented!();
}
