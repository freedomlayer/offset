use std::io;
use std::convert::TryFrom;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt, ByteOrder};

use common_capnp::{self, buffer128, buffer256, buffer512,
                    public_key, invoice_id, hash, dh_public_key, salt, signature,
                    rand_nonce, custom_u_int128, custom_int128, uid,
                    tcp_address_v4, tcp_address_v6, tcp_address, 
                    relay_address, index_server_address};

use crate::funder::messages::{InvoiceId, RelayAddress, 
    TcpAddress, TcpAddressV4, TcpAddressV6};
use crate::index_server::messages::IndexServerAddress;

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

        pub fn $read_func(from: &$capnp_type::Reader) -> Result<$native_type, capnp::Error>  {
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

pub fn read_custom_u_int128(from: &custom_u_int128::Reader) -> Result<u128, capnp::Error> {
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


pub fn read_custom_int128(from: &custom_int128::Reader) -> Result<i128, capnp::Error> {
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

pub fn read_relay_address(from: &relay_address::Reader) -> Result<RelayAddress, capnp::Error> {
    let public_key = read_public_key(&from.get_public_key()?)?;

    let address = match from.get_address()?.which()? {
        common_capnp::tcp_address::V4(res_tcp_address_v4) => {
            let tcp_address_v4 = res_tcp_address_v4?;
            let address_u32 = tcp_address_v4.get_address();
            let mut octets = [0u8; 4];
            BigEndian::write_u32(&mut octets, address_u32);

            let port = tcp_address_v4.get_port();

            TcpAddress::V4(TcpAddressV4 {
                octets,
                port,
            })
        },
        common_capnp::tcp_address::V6(res_tcp_address_v6) => {
            let tcp_address_v6 = res_tcp_address_v6?;
            let address_vec = read_buffer128(&tcp_address_v6.get_address()?);
            let mut address = [0u8; 16];
            address.copy_from_slice(&address_vec[..]); 

            let port = tcp_address_v6.get_port();

            let mut segments = [0u16; 8];
            for i in 0 .. 8 {
                segments[i] = ((address[2*i] as u16) << 8) | (address[2*i + 1] as u16);
            }

            TcpAddress::V6(TcpAddressV6 {
                segments,
                port,
            })
        },
    };

    Ok(RelayAddress {
        public_key,
        address,
    })
}

pub fn write_relay_address(from: &RelayAddress, to: &mut relay_address::Builder) {

    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    let tcp_address_builder = to.reborrow().init_address();

    match &from.address {
        TcpAddress::V4(tcp_address_v4) => {
            let mut tcp_address_v4_writer = tcp_address_builder.init_v4();
            let address_u32 = BigEndian::read_u32(&tcp_address_v4.octets);
            tcp_address_v4_writer.set_port(tcp_address_v4.port);
            tcp_address_v4_writer.set_address(address_u32);
        },
        TcpAddress::V6(tcp_address_v6) => {
            let mut tcp_address_v6_writer = tcp_address_builder.init_v6();
            tcp_address_v6_writer.set_port(tcp_address_v6.port);
            let mut address_builder = tcp_address_v6_writer.init_address();
            let mut address = [0u8; 16];
            for i in 0 .. 8usize {
                let segment = tcp_address_v6.segments[i];
                address[2*i]     = (segment >> 8) as u8;
                address[2*i + 1] = (segment & 0xff) as u8;
            }
            write_buffer128(&address, &mut address_builder);
        },
    }
}

pub fn read_index_server_address(from: &index_server_address::Reader) -> Result<IndexServerAddress, capnp::Error> {
    let public_key = read_public_key(&from.get_public_key()?)?;

    let address = match from.get_address()?.which()? {
        common_capnp::tcp_address::V4(res_tcp_address_v4) => {
            let tcp_address_v4 = res_tcp_address_v4?;
            let address_u32 = tcp_address_v4.get_address();
            let mut octets = [0u8; 4];
            BigEndian::write_u32(&mut octets, address_u32);

            let port = tcp_address_v4.get_port();

            TcpAddress::V4(TcpAddressV4 {
                octets,
                port,
            })
        },
        common_capnp::tcp_address::V6(res_tcp_address_v6) => {
            let tcp_address_v6 = res_tcp_address_v6?;
            let address_vec = read_buffer128(&tcp_address_v6.get_address()?);
            let mut address = [0u8; 16];
            address.copy_from_slice(&address_vec[..]); 

            let port = tcp_address_v6.get_port();

            let mut segments = [0u16; 8];
            for i in 0 .. 8 {
                segments[i] = ((address[2*i] as u16) << 8) | (address[2*i + 1] as u16);
            }

            TcpAddress::V6(TcpAddressV6 {
                segments,
                port,
            })
        },
    };

    Ok(IndexServerAddress {
        public_key,
        address,
    })
}

pub fn write_index_server_address(from: &IndexServerAddress, to: &mut index_server_address::Builder) {

    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    let tcp_address_builder = to.reborrow().init_address();

    match &from.address {
        TcpAddress::V4(tcp_address_v4) => {
            let mut tcp_address_v4_writer = tcp_address_builder.init_v4();
            let address_u32 = BigEndian::read_u32(&tcp_address_v4.octets);
            tcp_address_v4_writer.set_port(tcp_address_v4.port);
            tcp_address_v4_writer.set_address(address_u32);
        },
        TcpAddress::V6(tcp_address_v6) => {
            let mut tcp_address_v6_writer = tcp_address_builder.init_v6();
            tcp_address_v6_writer.set_port(tcp_address_v6.port);
            let mut address_builder = tcp_address_v6_writer.init_address();
            let mut address = [0u8; 16];
            for i in 0 .. 8usize {
                let segment = tcp_address_v6.segments[i];
                address[2*i]     = (segment >> 8) as u8;
                address[2*i + 1] = (segment & 0xff) as u8;
            }
            write_buffer128(&address, &mut address_builder);
        },
    }
}
