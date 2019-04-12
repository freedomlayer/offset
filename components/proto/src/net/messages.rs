use std::convert::TryFrom;
// use byteorder::{WriteBytesExt, BigEndian};
use crate::consts::MAX_NET_ADDRESS_LENGTH;
use common::canonical_serialize::CanonicalSerialize;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display)]
#[display(fmt = "{}", _0)]
pub struct NetAddress(String);

impl NetAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CanonicalSerialize for NetAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.0.canonical_serialize()
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
        Ok(NetAddress(address))
    }
}
