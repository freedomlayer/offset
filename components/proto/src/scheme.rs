use crate::funder::scheme::FunderScheme;
use crate::net::messages::NetAddress;
use crate::app_server::messages::{RelayAddress, NamedRelayAddress};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffstScheme;

impl FunderScheme for OffstScheme {
    type Address = Vec<RelayAddress<NetAddress>>;
    type NamedAddress = Vec<NamedRelayAddress<NetAddress>>;
 
    fn anonymize_address(named_address: Self::NamedAddress) -> Self::Address {
        named_address
            .into_iter()
            .map(|named_relay_address| {
                RelayAddress {
                    public_key: named_relay_address.public_key,
                    address: named_relay_address.address,
                }
            })
            .collect::<Vec<_>>()
    }

    /*
    fn default_named_address() -> Self::NamedAddress {
        Vec::new()
    }
    */
}

