use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

use derive_more::Display;

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

// use byteorder::{WriteBytesExt, BigEndian};
use crate::consts::MAX_NET_ADDRESS_LENGTH;
use common::canonical_serialize::CanonicalSerialize;

#[capnp_conv(crate::common_capnp::net_address)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display)]
#[display(fmt = "{}", address)]
pub struct NetAddress {
    address: String,
}

impl NetAddress {
    pub fn as_str(&self) -> &str {
        &self.address
    }
}

impl CanonicalSerialize for NetAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.address.canonical_serialize()
    }
}

#[derive(Debug)]
pub enum NetAddressError {
    AddressTooLong,
}

impl TryFrom<String> for NetAddress {
    type Error = NetAddressError;
    fn try_from(address: String) -> Result<Self, Self::Error> {
        if address.len() > MAX_NET_ADDRESS_LENGTH {
            return Err(NetAddressError::AddressTooLong);
        }
        Ok(NetAddress { address })
    }
}
