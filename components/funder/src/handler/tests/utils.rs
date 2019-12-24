use std::fmt::Debug;
use std::hash::Hash;

use identity::IdentityClient;

use crypto::rand::CryptoRandom;
use signature::canonical::CanonicalSerialize;

use proto::funder::messages::FunderOutgoingControl;
use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;

use crate::ephemeral::Ephemeral;
use crate::handler::handler::{funder_handle_message, FunderHandlerError, FunderHandlerOutput};
use crate::state::FunderState;
use crate::types::{FunderIncoming, FunderOutgoingComm};

const TEST_MAX_NODE_RELAYS: usize = 16;
const TEST_MAX_OPERATIONS_IN_BATCH: usize = 16;
const TEST_MAX_PENDING_USER_REQUESTS: usize = 16;

/// A helper function to quickly create a dummy NamedRelayAddress.
pub fn dummy_named_relay_address(index: u8) -> NamedRelayAddress<u32> {
    NamedRelayAddress {
        public_key: PublicKey::from(&[index; PublicKey::len()]),
        address: index as u32,
        name: format!("relay-{}", index),
    }
}

/// A helper function to quickly create a dummy RelayAddress.
pub fn dummy_relay_address(index: u8) -> RelayAddress<u32> {
    dummy_named_relay_address(index).into()
}

/// A helper function. Applies an incoming funder message, updating state and ephemeral
/// accordingly:
pub async fn apply_funder_incoming<'a, B, R>(
    funder_incoming: FunderIncoming<B>,
    state: &'a mut FunderState<B>,
    ephemeral: &'a mut Ephemeral,
    rng: &'a mut R,
    identity_client: &'a mut IdentityClient,
) -> Result<(Vec<FunderOutgoingComm<B>>, Vec<FunderOutgoingControl<B>>), FunderHandlerError>
where
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug + Hash + 'a,
    R: CryptoRandom + 'a,
{
    let funder_handler_output = funder_handle_message(
        identity_client,
        rng,
        state.clone(),
        ephemeral.clone(),
        TEST_MAX_NODE_RELAYS,
        TEST_MAX_OPERATIONS_IN_BATCH,
        TEST_MAX_PENDING_USER_REQUESTS,
        funder_incoming,
    )
    .await?;

    let FunderHandlerOutput {
        ephemeral_mutations,
        funder_mutations,
        outgoing_comms,
        outgoing_control,
    } = funder_handler_output;

    // Mutate FunderState according to the mutations:
    for mutation in &funder_mutations {
        state.mutate(mutation);
    }

    // Mutate Ephemeral according to the mutations:
    for mutation in &ephemeral_mutations {
        ephemeral.mutate(mutation);
    }

    Ok((outgoing_comms, outgoing_control))
}
