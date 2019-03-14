use std::fmt;
use std::convert::TryFrom;
// use byteorder::{WriteBytesExt, BigEndian};
use common::canonical_serialize::CanonicalSerialize;
use crate::consts::MAX_NET_ADDRESS_LENGTH;


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetAddress(String);

impl NetAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NetAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
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
