use std::io;
use std::convert::{TryFrom, TryInto};
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt, ByteOrder};

use common_capnp::{buffer128, buffer256, buffer512,
                    public_key, invoice_id, hash, dh_public_key, salt, signature,
                    rand_nonce, custom_u_int128, custom_int128, uid,
                    relay_address, receipt,
                    net_address, named_index_server};

use crate::serialize::SerializeError;
use crate::funder::messages::{InvoiceId, RelayAddress, Receipt};
use crate::index_server::messages::NamedIndexServer;
use crate::net::messages::NetAddress;

use crypto::identity::{PublicKey, Signature};
use crypto::dh::{DhPublicKey, Salt};
use crypto::uid::Uid;
use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;


/// Read the underlying bytes from given `CustomUInt128` reader.
fn read_buffer128(from: &buffer128::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec
}

/// Fill the components of `CustomUInt128` from given bytes.
fn write_buffer128(from: impl AsRef<[u8]>, to: &mut buffer128::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
}

/// Read the underlying bytes from given `CustomUInt256` reader.
fn read_buffer256(from: &buffer256::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x2()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x3()).unwrap();
    vec
}

/// Fill the components of `CustomUInt256` from given bytes.
fn write_buffer256(from: impl AsRef<[u8]>, to: &mut buffer256::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
    to.set_x2(reader.read_u64::<BigEndian>().unwrap());
    to.set_x3(reader.read_u64::<BigEndian>().unwrap());
}


/// Read the underlying bytes from given `CustomUInt512` reader.
fn read_buffer512(from: &buffer512::Reader) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.write_u64::<BigEndian>(from.get_x0()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x1()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x2()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x3()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x4()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x5()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x6()).unwrap();
    vec.write_u64::<BigEndian>(from.get_x7()).unwrap();
    vec
}

/// Fill the components of `CustomUInt512` from given bytes.
fn write_buffer512(from: impl AsRef<[u8]>, to: &mut buffer512::Builder) {
    let mut reader = io::Cursor::new(from.as_ref());
    to.set_x0(reader.read_u64::<BigEndian>().unwrap());
    to.set_x1(reader.read_u64::<BigEndian>().unwrap());
    to.set_x2(reader.read_u64::<BigEndian>().unwrap());
    to.set_x3(reader.read_u64::<BigEndian>().unwrap());
    to.set_x4(reader.read_u64::<BigEndian>().unwrap());
    to.set_x5(reader.read_u64::<BigEndian>().unwrap());
    to.set_x6(reader.read_u64::<BigEndian>().unwrap());
    to.set_x7(reader.read_u64::<BigEndian>().unwrap());
}

/// Define read and write functions for basic types
macro_rules! type_capnp_serde {
    ($capnp_type:ident, $native_type:ident, $read_func:ident, $write_func:ident, $inner_read_func:ident, $inner_write_func:ident) => {

        pub fn $read_func(from: &$capnp_type::Reader) -> Result<$native_type, SerializeError>  {
            let inner = from.get_inner()?;
            let data_bytes = &$inner_read_func(&inner);
            Ok($native_type::try_from(&data_bytes[..]).unwrap())
        }

        pub fn $write_func(from: &$native_type, to: &mut $capnp_type::Builder) {
            let mut inner = to.reborrow().get_inner().unwrap();
            $inner_write_func(from, &mut inner);
        }
    }
}



// 128 bits:
type_capnp_serde!(rand_nonce, RandValue, read_rand_nonce, write_rand_nonce, read_buffer128, write_buffer128);
type_capnp_serde!(uid, Uid, read_uid, write_uid, read_buffer128, write_buffer128);

// 256 bits:
type_capnp_serde!(public_key, PublicKey, read_public_key, write_public_key, read_buffer256, write_buffer256);
type_capnp_serde!(dh_public_key, DhPublicKey, read_dh_public_key, write_dh_public_key, read_buffer256, write_buffer256);
type_capnp_serde!(salt, Salt, read_salt, write_salt, read_buffer256, write_buffer256);
type_capnp_serde!(hash, HashResult, read_hash, write_hash, read_buffer256, write_buffer256);
type_capnp_serde!(invoice_id, InvoiceId, read_invoice_id, write_invoice_id, read_buffer256, write_buffer256);

// 512 bits:
type_capnp_serde!(signature, Signature, read_signature, write_signature, read_buffer512, write_buffer512);

pub fn read_custom_u_int128(from: &custom_u_int128::Reader) -> Result<u128, SerializeError> {
    let inner = from.get_inner()?;
    let data_bytes = read_buffer128(&inner);
    Ok(BigEndian::read_u128(&data_bytes))
}

pub fn write_custom_u_int128(from: u128, to: &mut custom_u_int128::Builder) {
    let mut inner = to.reborrow().get_inner().unwrap();
    let mut data_bytes = Vec::new();
    data_bytes.write_u128::<BigEndian>(from).unwrap();
    write_buffer128(&data_bytes, &mut inner);
}


pub fn read_custom_int128(from: &custom_int128::Reader) -> Result<i128, SerializeError> {
    let inner = from.get_inner()?;
    let data_bytes = read_buffer128(&inner);
    Ok(BigEndian::read_i128(&data_bytes))
}

pub fn write_custom_int128(from: i128, to: &mut custom_int128::Builder) {
    let mut inner = to.reborrow().get_inner().unwrap();
    let mut data_bytes = Vec::new();
    data_bytes.write_i128::<BigEndian>(from).unwrap();
    write_buffer128(&data_bytes, &mut inner);
}

pub fn read_net_address(from: &net_address::Reader) -> Result<NetAddress, SerializeError> {
    Ok(from.get_address()?.to_string().try_into()?)
}

pub fn write_net_address(from: &NetAddress, to: &mut net_address::Builder) {
    to.set_address(from.as_str());
}

pub fn read_relay_address(from: &relay_address::Reader) -> Result<RelayAddress, SerializeError> {
    Ok(RelayAddress {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: read_net_address(&from.get_address()?)?,
    })
}

pub fn write_relay_address(from: &RelayAddress, to: &mut relay_address::Builder) {
    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    write_net_address(&from.address,&mut to.reborrow().init_address());
}

pub fn read_named_index_server(from: &named_index_server::Reader) -> Result<NamedIndexServer<NetAddress>, SerializeError> {
    Ok(NamedIndexServer {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: read_net_address(&from.get_address()?)?,
        name: from.get_name()?.to_owned(),
    })
}

pub fn write_named_index_server(from: &NamedIndexServer<NetAddress>, to: &mut named_index_server::Builder) {
    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    write_net_address(&from.address,&mut to.reborrow().init_address());
    to.reborrow().set_name(&from.name);
}


/*
pub fn read_index_server_address(from: &index_server_address::Reader) -> Result<IndexServerAddress, SerializeError> {
    Ok(IndexServerAddress {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: from.get_address()?.to_owned().try_into()?,
    })
}

pub fn write_index_server_address(from: &IndexServerAddress, to: &mut index_server_address::Builder) {

    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    to.set_address(from.address.as_str());
}
*/


pub fn read_receipt(from: &receipt::Reader) -> Result<Receipt, SerializeError> {
    Ok(Receipt {
        response_hash: read_hash(&from.get_response_hash()?)?,
        invoice_id: read_invoice_id(&from.get_invoice_id()?)?,
        dest_payment: read_custom_u_int128(&from.get_dest_payment()?)?,
        signature: read_signature(&from.get_signature()?)?,
    })
}


pub fn write_receipt(from: &Receipt, to: &mut receipt::Builder) {
    write_hash(&from.response_hash, &mut to.reborrow().init_response_hash());
    write_invoice_id(&from.invoice_id, &mut to.reborrow().init_invoice_id());
    write_custom_u_int128(from.dest_payment, &mut to.reborrow().init_dest_payment());
    write_signature(&from.signature, &mut to.reborrow().init_signature());
}
