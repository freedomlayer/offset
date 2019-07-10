use identity::IdentityClient;
use std::fmt::Debug;

use signature::canonical::CanonicalSerialize;
use crypto::rand::CryptoRandom;

use proto::funder::messages::FunderOutgoingControl;

use crate::ephemeral::Ephemeral;
use crate::handler::handler::{funder_handle_message, FunderHandlerError, FunderHandlerOutput};
use crate::state::FunderState;
use crate::types::{FunderIncoming, FunderOutgoingComm};

const TEST_MAX_NODE_RELAYS: usize = 16;
const TEST_MAX_OPERATIONS_IN_BATCH: usize = 16;
const TEST_MAX_PENDING_USER_REQUESTS: usize = 16;

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
    B: Clone + PartialEq + Eq + CanonicalSerialize + Debug + 'a,
    R: CryptoRandom + 'a,
{
    let funder_handler_output = await!(funder_handle_message(
        identity_client,
        rng,
        state.clone(),
        ephemeral.clone(),
        TEST_MAX_NODE_RELAYS,
        TEST_MAX_OPERATIONS_IN_BATCH,
        TEST_MAX_PENDING_USER_REQUESTS,
        funder_incoming
    ))?;

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
