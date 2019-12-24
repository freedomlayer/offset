use std::fmt::Debug;
use std::hash::Hash;

use signature::canonical::CanonicalSerialize;

use crypto::rand::CryptoRandom;

use proto::app_server::messages::RelayAddress;
use proto::crypto::Uid;
use proto::funder::messages::FunderOutgoingControl;
use proto::report::messages::{FunderReportMutation, FunderReportMutations};

use identity::IdentityClient;

use crate::state::{FunderMutation, FunderState};

use crate::handler::handle_control::handle_control_message;
use crate::handler::handle_friend::{handle_friend_message, HandleFriendError};
use crate::handler::handle_init::handle_init;
use crate::handler::handle_liveness::{handle_liveness_message, HandleLivenessError};
use crate::handler::sender::create_friend_messages;
use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
use crate::handler::types::SendCommands;

use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::report::{ephemeral_mutation_to_report_mutations, funder_mutation_to_report_mutations};
use crate::types::{ChannelerConfig, FunderIncoming, FunderIncomingComm, FunderOutgoingComm};

#[derive(Debug)]
pub enum FunderHandlerError {
    // HandleControlError(HandleControlError),
    HandleFriendError(HandleFriendError),
    HandleLivenessError(HandleLivenessError),
}

pub struct FunderHandlerOutput<B>
where
    B: Clone,
{
    pub funder_mutations: Vec<FunderMutation<B>>,
    pub ephemeral_mutations: Vec<EphemeralMutation>,
    pub outgoing_comms: Vec<FunderOutgoingComm<B>>,
    pub outgoing_control: Vec<FunderOutgoingControl<B>>,
}

type FunderHandleIncomingOutput<B> = (
    SendCommands,
    Vec<FunderOutgoingControl<B>>,
    Vec<ChannelerConfig<RelayAddress<B>>>,
    Option<Uid>,
);
pub fn funder_handle_incoming<B, R>(
    mut m_state: &mut MutableFunderState<B>,
    mut m_ephemeral: &mut MutableEphemeral,
    rng: &R,
    max_node_relays: usize,
    max_pending_user_requests: usize,
    funder_incoming: FunderIncoming<B>,
) -> Result<FunderHandleIncomingOutput<B>, FunderHandlerError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    let mut send_commands = SendCommands::new();
    let mut outgoing_control = Vec::new();
    let mut outgoing_channeler_config = Vec::new();

    let opt_app_request_id = match funder_incoming {
        FunderIncoming::Init => {
            handle_init(&m_state, &mut outgoing_channeler_config);
            None
        }

        FunderIncoming::Control(funder_incoming_control) => {
            // Even if an error occurs, we must return an indication to the
            // user that the control request was received.
            if let Err(e) = handle_control_message(
                &mut m_state,
                &mut m_ephemeral,
                &mut send_commands,
                &mut outgoing_control,
                &mut outgoing_channeler_config,
                rng,
                max_node_relays,
                max_pending_user_requests,
                funder_incoming_control.funder_control,
            ) {
                warn!("handle_control_error(): {:?}", e);
            }
            Some(funder_incoming_control.app_request_id)
        }

        FunderIncoming::Comm(incoming_comm) => {
            match incoming_comm {
                FunderIncomingComm::Liveness(liveness_message) => handle_liveness_message::<B, R>(
                    &mut m_state,
                    &mut m_ephemeral,
                    &mut send_commands,
                    &mut outgoing_control,
                    rng,
                    liveness_message,
                )
                .map_err(FunderHandlerError::HandleLivenessError)?,

                FunderIncomingComm::Friend((origin_public_key, friend_message)) => {
                    handle_friend_message(
                        &mut m_state,
                        &mut m_ephemeral,
                        &mut send_commands,
                        &mut outgoing_control,
                        &mut outgoing_channeler_config,
                        rng,
                        &origin_public_key,
                        friend_message,
                    )
                    .map_err(FunderHandlerError::HandleFriendError)?
                }
            };
            None
        }
    };

    Ok((
        send_commands,
        outgoing_control,
        outgoing_channeler_config,
        opt_app_request_id,
    ))
}

fn create_report_mutations<B>(
    initial_state: FunderState<B>,
    funder_mutations: &[FunderMutation<B>],
    ephemeral_mutations: &[EphemeralMutation],
) -> Vec<FunderReportMutation<B>>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    let mut report_mutations = Vec::new();
    let mut running_state = initial_state;
    for funder_mutation in funder_mutations {
        report_mutations.extend(funder_mutation_to_report_mutations(
            funder_mutation,
            &running_state,
        ));
        running_state.mutate(funder_mutation);
    }

    // At this point the running_state is the final funder_state:
    let funder_state = running_state;

    for ephemeral_mutation in ephemeral_mutations {
        report_mutations.extend(ephemeral_mutation_to_report_mutations::<B>(
            ephemeral_mutation,
            &funder_state,
        ));
    }

    report_mutations
}

pub async fn funder_handle_message<'a, B, R>(
    identity_client: &'a mut IdentityClient,
    rng: &'a R,
    funder_state: FunderState<B>,
    funder_ephemeral: Ephemeral,
    max_node_relays: usize,
    max_operations_in_batch: usize,
    max_pending_user_requests: usize,
    funder_incoming: FunderIncoming<B>,
) -> Result<FunderHandlerOutput<B>, FunderHandlerError>
where
    B: 'a + Clone + PartialEq + Eq + CanonicalSerialize + Debug + Hash,
    R: CryptoRandom + 'a,
{
    let mut m_state = MutableFunderState::new(funder_state);
    let mut m_ephemeral = MutableEphemeral::new(funder_ephemeral);
    let mut outgoing_comms = Vec::new();

    let (send_commands, handle_outgoing_control, outgoing_channeler_config, opt_app_request_id) =
        funder_handle_incoming(
            &mut m_state,
            &mut m_ephemeral,
            rng,
            max_node_relays,
            max_pending_user_requests,
            funder_incoming,
        )?;

    for channeler_config in outgoing_channeler_config {
        outgoing_comms.push(FunderOutgoingComm::ChannelerConfig(channeler_config));
    }

    // Sign all unsigned responses and then queue them as mutations
    m_state.sign_responses(identity_client, rng).await;

    // Send all possible messages according to SendCommands
    // TODO: Maybe we should output outgoing_comms instead of friend_messages and
    // outgoing_channeler_config. When we merge the two, we might be out of order!
    let (friend_messages, outgoing_channeler_config) = create_friend_messages(
        &mut m_state,
        m_ephemeral.ephemeral(),
        &send_commands,
        max_operations_in_batch,
        identity_client,
        rng,
    )
    .await;

    for channeler_config in outgoing_channeler_config {
        outgoing_comms.push(FunderOutgoingComm::ChannelerConfig(channeler_config));
    }

    for friend_message in friend_messages {
        outgoing_comms.push(FunderOutgoingComm::FriendMessage(friend_message));
    }

    let (initial_state, funder_mutations, _state) = m_state.done();
    let (ephemeral_mutations, _ephemeral) = m_ephemeral.done();

    // Add reports:
    let report_mutations = create_report_mutations(
        initial_state,
        &funder_mutations[..],
        &ephemeral_mutations[..],
    );

    let funder_report_mutations = FunderReportMutations {
        opt_app_request_id,
        mutations: report_mutations,
    };

    let mut outgoing_control = Vec::new();
    if !funder_report_mutations.mutations.is_empty()
        || funder_report_mutations.opt_app_request_id.is_some()
    {
        outgoing_control.push(FunderOutgoingControl::ReportMutations(
            funder_report_mutations,
        ));
    }

    // We always send the report mutations first through the outgoing control:
    outgoing_control.extend(handle_outgoing_control);

    Ok(FunderHandlerOutput {
        funder_mutations,
        ephemeral_mutations,
        outgoing_comms,
        outgoing_control,
    })
}
