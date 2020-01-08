use std::convert::TryFrom;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use derive_more::Display;

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use common::ser_utils::ser_string;

use crate::consts::MAX_NET_ADDRESS_LENGTH;

#[capnp_conv(crate::common_capnp::net_address)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display)]
#[display(fmt = "{}", address)]
pub struct NetAddress {
    #[serde(with = "ser_string")]
    address: String,
}

impl NetAddress {
    pub fn as_str(&self) -> &str {
        &self.address
    }
}

impl quickcheck::Arbitrary for NetAddress {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> NetAddress {
        let size = rand::Rng::gen_range(g, 1, MAX_NET_ADDRESS_LENGTH);
        let mut s = String::with_capacity(size);
        for _ in 0..size {
            let new_char = rand::seq::SliceRandom::choose(&['a', 'b', 'c', 'd'][..], g)
                .unwrap()
                .to_owned();
            s.push(new_char);
        }
        NetAddress { address: s }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = NetAddress>> {
        // Shrink a string by shrinking a vector of its characters.
        let chars: Vec<char> = self.address.chars().collect();
        Box::new(chars.shrink().map(|x| NetAddress {
            address: x.into_iter().collect::<String>(),
        }))
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
