use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use std::convert::{TryFrom, TryInto};
use std::io;

use common::int_convert::usize_to_u32;

use common_capnp::{
    buffer128, buffer256, buffer512, commit, custom_int128, custom_u_int128, dh_public_key, hash,
    hashed_lock, invoice_id, multi_commit, named_index_server_address, named_relay_address,
    net_address, payment_id, plain_lock, public_key, rand_nonce, rate, receipt, relay_address,
    salt, signature, uid,
};

use crate::app_server::messages::{NamedRelayAddress, RelayAddress};
use crate::funder::messages::{Commit, MultiCommit, Rate, Receipt};
use crate::index_server::messages::NamedIndexServerAddress;
use crate::net::messages::NetAddress;
use crate::serialize::SerializeError;

use crypto::crypto_rand::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::hash::HashResult;
use crypto::hash_lock::{HashedLock, PlainLock};
use crypto::identity::{PublicKey, Signature};
use crypto::invoice_id::InvoiceId;
use crypto::payment_id::PaymentId;
use crypto::uid::Uid;

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
        pub fn $read_func(from: &$capnp_type::Reader) -> Result<$native_type, SerializeError> {
            let inner = from.get_inner()?;
            let data_bytes = &$inner_read_func(&inner);
            Ok($native_type::try_from(&data_bytes[..]).unwrap())
        }

        pub fn $write_func(from: &$native_type, to: &mut $capnp_type::Builder) {
            let mut inner = to.reborrow().get_inner().unwrap();
            $inner_write_func(from, &mut inner);
        }
    };
}

// 128 bits:
type_capnp_serde!(
    rand_nonce,
    RandValue,
    read_rand_nonce,
    write_rand_nonce,
    read_buffer128,
    write_buffer128
);
type_capnp_serde!(
    uid,
    Uid,
    read_uid,
    write_uid,
    read_buffer128,
    write_buffer128
);

// 256 bits:
type_capnp_serde!(
    public_key,
    PublicKey,
    read_public_key,
    write_public_key,
    read_buffer256,
    write_buffer256
);
type_capnp_serde!(
    dh_public_key,
    DhPublicKey,
    read_dh_public_key,
    write_dh_public_key,
    read_buffer256,
    write_buffer256
);
type_capnp_serde!(
    salt,
    Salt,
    read_salt,
    write_salt,
    read_buffer256,
    write_buffer256
);
type_capnp_serde!(
    hash,
    HashResult,
    read_hash,
    write_hash,
    read_buffer256,
    write_buffer256
);

type_capnp_serde!(
    invoice_id,
    InvoiceId,
    read_invoice_id,
    write_invoice_id,
    read_buffer256,
    write_buffer256
);

type_capnp_serde!(
    payment_id,
    PaymentId,
    read_payment_id,
    write_payment_id,
    read_buffer128,
    write_buffer128
);

// 512 bits:
type_capnp_serde!(
    signature,
    Signature,
    read_signature,
    write_signature,
    read_buffer512,
    write_buffer512
);

// 256 bits:
type_capnp_serde!(
    plain_lock,
    PlainLock,
    read_plain_lock,
    write_plain_lock,
    read_buffer256,
    write_buffer256
);

// 256 bits:
type_capnp_serde!(
    hashed_lock,
    HashedLock,
    read_hashed_lock,
    write_hashed_lock,
    read_buffer256,
    write_buffer256
);

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

pub fn read_relay_address(
    from: &relay_address::Reader,
) -> Result<RelayAddress<NetAddress>, SerializeError> {
    Ok(RelayAddress {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: read_net_address(&from.get_address()?)?,
    })
}

pub fn write_relay_address(from: &RelayAddress<NetAddress>, to: &mut relay_address::Builder) {
    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    write_net_address(&from.address, &mut to.reborrow().init_address());
}

pub fn read_named_relay_address(
    from: &named_relay_address::Reader,
) -> Result<NamedRelayAddress<NetAddress>, SerializeError> {
    Ok(NamedRelayAddress {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: read_net_address(&from.get_address()?)?,
        name: from.get_name()?.to_string(),
    })
}

pub fn write_named_relay_address(
    from: &NamedRelayAddress<NetAddress>,
    to: &mut named_relay_address::Builder,
) {
    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    write_net_address(&from.address, &mut to.reborrow().init_address());
    to.reborrow().set_name(&from.name);
}

pub fn read_named_index_server_address(
    from: &named_index_server_address::Reader,
) -> Result<NamedIndexServerAddress<NetAddress>, SerializeError> {
    Ok(NamedIndexServerAddress {
        public_key: read_public_key(&from.get_public_key()?)?,
        address: read_net_address(&from.get_address()?)?,
        name: from.get_name()?.to_owned(),
    })
}

pub fn write_named_index_server_address(
    from: &NamedIndexServerAddress<NetAddress>,
    to: &mut named_index_server_address::Builder,
) {
    write_public_key(&from.public_key, &mut to.reborrow().init_public_key());
    write_net_address(&from.address, &mut to.reborrow().init_address());
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
        src_plain_lock: read_plain_lock(&from.get_src_plain_lock()?)?,
        dest_plain_lock: read_plain_lock(&from.get_dest_plain_lock()?)?,
        dest_payment: read_custom_u_int128(&from.get_dest_payment()?)?,
        total_dest_payment: read_custom_u_int128(&from.get_total_dest_payment()?)?,
        signature: read_signature(&from.get_signature()?)?,
    })
}

pub fn write_receipt(from: &Receipt, to: &mut receipt::Builder) {
    write_hash(&from.response_hash, &mut to.reborrow().init_response_hash());
    write_invoice_id(&from.invoice_id, &mut to.reborrow().init_invoice_id());
    write_plain_lock(
        &from.src_plain_lock,
        &mut to.reborrow().init_src_plain_lock(),
    );
    write_plain_lock(
        &from.dest_plain_lock,
        &mut to.reborrow().init_dest_plain_lock(),
    );
    write_custom_u_int128(from.dest_payment, &mut to.reborrow().init_dest_payment());
    write_custom_u_int128(
        from.total_dest_payment,
        &mut to.reborrow().init_total_dest_payment(),
    );
    write_signature(&from.signature, &mut to.reborrow().init_signature());
}

pub fn read_commit(from: &commit::Reader) -> Result<Commit, SerializeError> {
    Ok(Commit {
        response_hash: read_hash(&from.get_response_hash()?)?,
        dest_payment: read_custom_u_int128(&from.get_dest_payment()?)?,
        src_plain_lock: read_plain_lock(&from.get_src_plain_lock()?)?,
        dest_hashed_lock: read_hashed_lock(&from.get_dest_hashed_lock()?)?,
        signature: read_signature(&from.get_signature()?)?,
    })
}

pub fn write_commit(from: &Commit, to: &mut commit::Builder) {
    write_hash(&from.response_hash, &mut to.reborrow().init_response_hash());
    write_custom_u_int128(from.dest_payment, &mut to.reborrow().init_dest_payment());
    write_plain_lock(
        &from.src_plain_lock,
        &mut to.reborrow().init_src_plain_lock(),
    );
    write_hashed_lock(
        &from.dest_hashed_lock,
        &mut to.reborrow().init_dest_hashed_lock(),
    );
    write_signature(&from.signature, &mut to.reborrow().init_signature());
}

pub fn read_multi_commit(from: &multi_commit::Reader) -> Result<MultiCommit, SerializeError> {
    let mut commits = Vec::new();
    for commit_reader in from.get_commits()? {
        commits.push(read_commit(&commit_reader)?);
    }

    Ok(MultiCommit {
        invoice_id: read_invoice_id(&from.get_invoice_id()?)?,
        total_dest_payment: read_custom_u_int128(&from.get_total_dest_payment()?)?,
        commits,
    })
}

pub fn write_multi_commit(from: &MultiCommit, to: &mut multi_commit::Builder) {
    write_invoice_id(&from.invoice_id, &mut to.reborrow().init_invoice_id());
    write_custom_u_int128(
        from.total_dest_payment,
        &mut to.reborrow().init_total_dest_payment(),
    );

    let mut commits_builder = to
        .reborrow()
        .init_commits(usize_to_u32(from.commits.len()).unwrap());

    for (index, commit) in from.commits.iter().enumerate() {
        let mut commit_builder = commits_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_commit(commit, &mut commit_builder);
    }
}

pub fn read_rate(from: &rate::Reader) -> Result<Rate, SerializeError> {
    Ok(Rate {
        mul: from.get_mul(),
        add: from.get_add(),
    })
}

pub fn write_rate(from: &Rate, to: &mut rate::Builder) {
    to.reborrow().set_mul(from.mul);
    to.reborrow().set_add(from.add);
}
