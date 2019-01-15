use std::fmt::Debug;
use identity::IdentityClient;

use crypto::uid::Uid;
use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use common::canonical_serialize::CanonicalSerialize;
use proto::funder::messages::{FunderOutgoingControl, 
    RequestSendFunds, FreezeLink};
use proto::funder::report::FunderReportMutation;

use crate::state::{FunderState, FunderMutation};

use crate::handler::handle_control::{handle_control_message, HandleControlError};
use crate::handler::handle_friend::{handle_friend_message, HandleFriendError};
use crate::handler::handle_liveness::{handle_liveness_message, HandleLivenessError};
use crate::handler::handle_init::handle_init;
use crate::handler::sender::{SendCommands, create_friend_messages};

use crate::types::{FunderIncoming,
    FunderOutgoingComm, FunderIncomingComm, ChannelerConfig};
use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::friend::ChannelStatus;
use crate::report::{funder_mutation_to_report_mutations, 
    ephemeral_mutation_to_report_mutations};


pub struct MutableFunderState<A: Clone> {
    initial_state: FunderState<A>,
    state: FunderState<A>,
    mutations: Vec<FunderMutation<A>>,
}

impl<A> MutableFunderState<A> 
where
    A: CanonicalSerialize + Clone,
{
    pub fn new(state: FunderState<A>) -> Self {
        MutableFunderState {
            initial_state: state.clone(),
            state,
            mutations: Vec::new(),
        }
    }

    pub fn mutate(&mut self, mutation: FunderMutation<A>) {
        self.state.mutate(&mutation);
        self.mutations.push(mutation);
    }

    pub fn state(&self) -> &FunderState<A> {
        &self.state
    }

    pub fn done(self) -> (FunderState<A>, Vec<FunderMutation<A>>, FunderState<A>) {
        (self.initial_state, self.mutations, self.state)
    }
}

pub struct MutableEphemeral {
    ephemeral: Ephemeral,
    mutations: Vec<EphemeralMutation>,
}

impl MutableEphemeral {
    pub fn new(ephemeral: Ephemeral) -> Self {
        MutableEphemeral {
            ephemeral,
            mutations: Vec::new(),
        }
    }
    pub fn mutate(&mut self, mutation: EphemeralMutation) {
        self.ephemeral.mutate(&mutation);
        self.mutations.push(mutation);
    }

    pub fn ephemeral(&self) -> &Ephemeral {
        &self.ephemeral
    }

    pub fn done(self) -> (Vec<EphemeralMutation>, Ephemeral) {
        (self.mutations, self.ephemeral)
    }
}

#[derive(Debug)]
pub enum FunderHandlerError {
    HandleControlError(HandleControlError),
    HandleFriendError(HandleFriendError),
    HandleLivenessError(HandleLivenessError),
}

pub struct FunderHandlerOutput<A: Clone> {
    pub funder_mutations: Vec<FunderMutation<A>>,
    pub ephemeral_mutations: Vec<EphemeralMutation>,
    pub outgoing_comms: Vec<FunderOutgoingComm<A>>,
    pub outgoing_control: Vec<FunderOutgoingControl<A>>,
}


pub fn add_local_freezing_link<A>(state: &FunderState<A>,
                               request_send_funds: &mut RequestSendFunds) 
where
    A: CanonicalSerialize + Clone,
{

    let index = request_send_funds.route.pk_to_index(&state.local_public_key)
        .unwrap();
    assert_eq!(request_send_funds.freeze_links.len(), index);

    let next_index = index.checked_add(1).unwrap();
    let next_pk = request_send_funds.route.index_to_pk(next_index).unwrap();

    let opt_prev_pk = match index.checked_sub(1) {
        Some(prev_index) =>
            Some(request_send_funds.route.index_to_pk(prev_index).unwrap()),
        None => None,
    };

    let funder_freeze_link = FreezeLink {
        shared_credits: state.friends.get(&next_pk).unwrap().get_shared_credits(),
        usable_ratio: state.get_usable_ratio(opt_prev_pk, next_pk),
    };

    // Add our freeze link
    request_send_funds.freeze_links.push(funder_freeze_link);

}


/// Find the originator of a pending local request.
/// This should be a pending remote request at some other friend.
/// Returns the public key of a friend. If we are the origin of this request, the function returns None.
///
/// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
/// between request_id and (friend_public_key, friend).
pub fn find_request_origin<'a, A>(state: &'a FunderState<A>,
                                  request_id: &Uid) -> Option<&'a PublicKey> 
where
    A: CanonicalSerialize + Clone,
{
    for (friend_public_key, friend) in &state.friends {
        match &friend.channel_status {
            ChannelStatus::Inconsistent(_) => continue,
            ChannelStatus::Consistent(token_channel) => {
                if token_channel
                    .get_mutual_credit()
                    .state()
                    .pending_requests
                    .pending_remote_requests
                    .contains_key(request_id) {
                        return Some(friend_public_key)
                }
            },
        }
    }
    None
}

pub fn is_friend_ready<A>(state: &FunderState<A>, 
                          ephemeral: &Ephemeral,
                          friend_public_key: &PublicKey) -> bool 
where
    A: CanonicalSerialize + Clone,
{

    let friend = state.friends.get(friend_public_key).unwrap();
    if !ephemeral.liveness.is_online(friend_public_key) {
        return false;
    }

    // Make sure that the channel is consistent:
    let token_channel = match &friend.channel_status {
        ChannelStatus::Inconsistent(_) => return false,
        ChannelStatus::Consistent(token_channel) => token_channel,
    };

    // Make sure that the remote side has open requests:
    token_channel
        .get_mutual_credit()
        .state()
        .requests_status.remote
        .is_open()
}

pub fn funder_handle_incoming<A,R>(mut m_state: &mut MutableFunderState<A>,
                                   mut m_ephemeral: &mut MutableEphemeral,
                                   rng: &R,
                                   funder_incoming: FunderIncoming<A>)
                                    -> Result<(SendCommands, 
                                        Vec<FunderOutgoingControl<A>>, 
                                        Vec<ChannelerConfig<A>>), FunderHandlerError>

where
    A: CanonicalSerialize + Clone + Eq,
    R: CryptoRandom,
{
    let mut send_commands = SendCommands::new();
    let mut outgoing_control = Vec::new();
    let mut outgoing_channeler_config = Vec::new();

    match funder_incoming {
        FunderIncoming::Init =>  {
            handle_init(&m_state, &mut outgoing_channeler_config);
        },

        FunderIncoming::Control(control_message) =>
            handle_control_message(&mut m_state, 
                                   &mut m_ephemeral,
                                   &mut send_commands,
                                   &mut outgoing_control,
                                   &mut outgoing_channeler_config,
                                   control_message)
                .map_err(FunderHandlerError::HandleControlError)?,

        FunderIncoming::Comm(incoming_comm) => {
            match incoming_comm {
                FunderIncomingComm::Liveness(liveness_message) =>
                    handle_liveness_message::<A>(&mut m_state, 
                                            &mut m_ephemeral, 
                                            &mut send_commands,
                                            &mut outgoing_control,
                                            liveness_message)
                        .map_err(FunderHandlerError::HandleLivenessError)?,

                FunderIncomingComm::Friend((origin_public_key, friend_message)) => 
                    handle_friend_message(&mut m_state, 
                                          &mut m_ephemeral,
                                          &mut send_commands,
                                          &mut outgoing_control,
                                          rng,
                                          &origin_public_key, 
                                          friend_message)
                        .map_err(FunderHandlerError::HandleFriendError)?,
            }
        },
    };

    Ok((send_commands, outgoing_control, outgoing_channeler_config))

}

fn create_report_mutations<A>(initial_state: FunderState<A>,
                           funder_mutations: &[FunderMutation<A>],
                           ephemeral_mutations: &[EphemeralMutation]) -> Vec<FunderReportMutation<A>> 
where
    A: CanonicalSerialize + Clone,
{

    let mut report_mutations = Vec::new();
    let mut running_state = initial_state;
    for funder_mutation in funder_mutations {
        report_mutations.extend(funder_mutation_to_report_mutations(funder_mutation, &running_state));
        running_state.mutate(funder_mutation);
    }
    
    for ephemeral_mutation in ephemeral_mutations {
        report_mutations.extend(ephemeral_mutation_to_report_mutations(ephemeral_mutation));
    }

    report_mutations

}


pub async fn funder_handle_message<'a, A,R>(
                      identity_client: &'a mut IdentityClient,
                      rng: &'a R,
                      funder_state: FunderState<A>,
                      funder_ephemeral: Ephemeral,
                      max_operations_in_batch: usize,
                      funder_incoming: FunderIncoming<A>) 
        -> Result<FunderHandlerOutput<A>, FunderHandlerError> 
where
    A: CanonicalSerialize + Clone + Debug + Eq + 'a,
    R: CryptoRandom + 'a,
{

    let mut m_state = MutableFunderState::new(funder_state);
    let mut m_ephemeral = MutableEphemeral::new(funder_ephemeral);
    let mut outgoing_comms = Vec::new();

    let (send_commands, mut outgoing_control, outgoing_channeler_config) = 
        funder_handle_incoming(&mut m_state, 
                               &mut m_ephemeral, 
                               rng, 
                               funder_incoming)?;

    for channeler_config in outgoing_channeler_config {
        outgoing_comms.push(
            FunderOutgoingComm::ChannelerConfig(channeler_config));
    }

    // Send all possible messages according to SendCommands
    let friend_messages = await!(create_friend_messages(
                                      &mut m_state,
                                      &mut m_ephemeral,
                                      &send_commands,
                                      max_operations_in_batch,
                                      identity_client,
                                      rng));

    for friend_message in friend_messages {
        outgoing_comms.push(FunderOutgoingComm::FriendMessage(friend_message));
    }

    // Add reports:
    let (initial_state, funder_mutations, _state) = m_state.done();
    let (ephemeral_mutations, _ephemeral) = m_ephemeral.done();
    let report_mutations = create_report_mutations(initial_state, 
                            &funder_mutations[..], 
                            &ephemeral_mutations[..]);

    if !report_mutations.is_empty() {
        outgoing_control.push(FunderOutgoingControl::ReportMutations(report_mutations));
    }

    Ok(FunderHandlerOutput {
        funder_mutations,
        ephemeral_mutations,
        outgoing_comms,
        outgoing_control,
    })
}

