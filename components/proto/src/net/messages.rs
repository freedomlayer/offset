use std::convert::TryFrom;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use derive_more::Display;

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use common::ser_utils::SerString;

use crate::consts::MAX_NET_ADDRESS_LENGTH;

#[capnp_conv(crate::common_capnp::net_address)]
#[derive(
    Arbitrary, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display,
)]
#[display(fmt = "{}", address)]
pub struct NetAddress {
    #[serde(with = "SerString")]
    address: String,
}

impl NetAddress {
    pub fn as_str(&self) -> &str {
        &self.address
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

impl FromStr for NetAddress {
    type Err = NetAddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_NET_ADDRESS_LENGTH {
            return Err(NetAddressError::AddressTooLong);
        }
        Ok(NetAddress {
            address: s.to_owned(),
        })
    }
}
