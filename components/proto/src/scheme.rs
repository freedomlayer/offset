use common::mutable_state::MutableState;

use crate::funder::scheme::FunderScheme;
use crate::net::messages::NetAddress;
use crate::app_server::messages::{RelayAddress, NamedRelayAddress, NamedRelaysMutation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffstScheme;

impl<B> MutableState for Vec<NamedRelayAddress<B>> {
    type Mutation = NamedRelaysMutation<B>;
    type MutateError = !;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            NamedRelaysMutation::AddRelay(named_relay_address) => {
                // First remove, to avoid duplicates:
                self.retain(|cur_named_relay_address| 
                            cur_named_relay_address.public_key != named_relay_address.public_key);
                self.push(named_relay_address.clone());
            },
            NamedRelaysMutation::RemoveRelay(public_key) => {
                self.retain(|cur_named_relay_address| 
                            &cur_named_relay_address.public_key != public_key)
            },
        };
        Ok(())
    }
}

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

