use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use derive_more::Display;

use crate::consts::MAX_NET_ADDRESS_LENGTH;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
#[display(fmt = "{}", address)]
pub struct NetAddress {
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

impl Serialize for NetAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.address)
    }
}

struct NetAddressVisitor;

impl<'de> Visitor<'de> for NetAddressVisitor {
    type Value = NetAddress;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("NetAddress string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let net_address = NetAddress::try_from(value.to_owned())
            .map_err(|e| E::custom(format!("Invalid NetAddress string {:?}: {}", e, value)))?;
        Ok(net_address)
    }
}

impl<'de> Deserialize<'de> for NetAddress {
    fn deserialize<D>(deserializer: D) -> Result<NetAddress, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NetAddressVisitor)
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
