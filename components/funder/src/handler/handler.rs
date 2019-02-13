use crypto::uid::Uid;
use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use proto::funder::messages::FunderOutgoingControl;
use proto::funder::scheme::FunderScheme;
use proto::report::messages::FunderReportMutation;

use identity::IdentityClient;

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




pub struct MutableFunderState<FS: FunderScheme> {
    initial_state: FunderState<FS>,
    state: FunderState<FS>,
    mutations: Vec<FunderMutation<FS>>,
}

impl<FS: FunderScheme> MutableFunderState<FS> {
    pub fn new(state: FunderState<FS>) -> Self {
        MutableFunderState {
            initial_state: state.clone(),
            state,
            mutations: Vec::new(),
        }
    }

    pub fn mutate(&mut self, mutation: FunderMutation<FS>) {
        self.state.mutate(&mutation);
        self.mutations.push(mutation);
    }

    pub fn state(&self) -> &FunderState<FS> {
        &self.state
    }

    pub fn done(self) -> (FunderState<FS>, Vec<FunderMutation<FS>>, FunderState<FS>) {
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

pub struct FunderHandlerOutput<FS: FunderScheme> {
    pub funder_mutations: Vec<FunderMutation<FS>>,
    pub ephemeral_mutations: Vec<EphemeralMutation>,
    pub outgoing_comms: Vec<FunderOutgoingComm<FS>>,
    pub outgoing_control: Vec<FunderOutgoingControl<FS>>,
}


/// Find the originator of a pending local request.
/// This should be a pending remote request at some other friend.
/// Returns the public key of a friend. If we are the origin of this request, the function returns None.
///
/// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
/// between request_id and (friend_public_key, friend).
pub fn find_request_origin<'a, FS:FunderScheme>(state: &'a FunderState<FS>,
                                  request_id: &Uid) -> Option<&'a PublicKey> {
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

pub fn is_friend_ready<FS:FunderScheme>(state: &FunderState<FS>, 
                          ephemeral: &Ephemeral,
                          friend_public_key: &PublicKey) -> bool {

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

pub fn funder_handle_incoming<FS,R>(mut m_state: &mut MutableFunderState<FS>,
                                   mut m_ephemeral: &mut MutableEphemeral,
                                   rng: &R,
                                   max_pending_user_requests: usize,
                                   funder_incoming: FunderIncoming<FS>)
                                    -> Result<(SendCommands, 
                                        Vec<FunderOutgoingControl<FS>>, 
                                        Vec<ChannelerConfig<FS::Address>>), FunderHandlerError>

where
    FS: FunderScheme,
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
                                   max_pending_user_requests,
                                   control_message)
                .map_err(FunderHandlerError::HandleControlError)?,

        FunderIncoming::Comm(incoming_comm) => {
            match incoming_comm {
                FunderIncomingComm::Liveness(liveness_message) =>
                    handle_liveness_message::<FS>(&mut m_state, 
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
                                          &mut outgoing_channeler_config,
                                          rng,
                                          &origin_public_key, 
                                          friend_message)
                        .map_err(FunderHandlerError::HandleFriendError)?,
            }
        },
    };

    Ok((send_commands, outgoing_control, outgoing_channeler_config))

}

fn create_report_mutations<FS>(initial_state: FunderState<FS>,
                           funder_mutations: &[FunderMutation<FS>],
                           ephemeral_mutations: &[EphemeralMutation]) 
    -> Vec<FunderReportMutation<FS::Address, FS::NamedAddress>> 
where
    FS: FunderScheme,
{

    let mut report_mutations = Vec::new();
    let mut running_state = initial_state;
    for funder_mutation in funder_mutations {
        report_mutations.extend(funder_mutation_to_report_mutations(funder_mutation, &running_state));
        running_state.mutate(funder_mutation);
    }
    
    for ephemeral_mutation in ephemeral_mutations {
        report_mutations.extend(ephemeral_mutation_to_report_mutations::<FS>(ephemeral_mutation));
    }

    report_mutations

}


pub async fn funder_handle_message<'a,FS,R>(
                      identity_client: &'a mut IdentityClient,
                      rng: &'a R,
                      funder_state: FunderState<FS>,
                      funder_ephemeral: Ephemeral,
                      max_operations_in_batch: usize,
                      max_pending_user_requests: usize,
                      funder_incoming: FunderIncoming<FS>) 
        -> Result<FunderHandlerOutput<FS>, FunderHandlerError> 
where
    FS: FunderScheme + 'a,
    R: CryptoRandom + 'a,
{

    let mut m_state = MutableFunderState::new(funder_state);
    let mut m_ephemeral = MutableEphemeral::new(funder_ephemeral);
    let mut outgoing_comms = Vec::new();

    let (send_commands, mut outgoing_control, outgoing_channeler_config) = 
        funder_handle_incoming(&mut m_state, 
                               &mut m_ephemeral, 
                               rng, 
                               max_pending_user_requests,
                               funder_incoming)?;

    for channeler_config in outgoing_channeler_config {
        outgoing_comms.push(
            FunderOutgoingComm::ChannelerConfig(channeler_config));
    }


    // Send all possible messages according to SendCommands
    // TODO: Maybe we should output outgoing_comms instead of friend_messages and
    // outgoing_channeler_config. When we merge the two, we might be out of order!
    let (friend_messages, outgoing_channeler_config) = await!(create_friend_messages(
                                      &mut m_state,
                                      &send_commands,
                                      max_operations_in_batch,
                                      identity_client,
                                      rng));

    for channeler_config in outgoing_channeler_config {
        outgoing_comms.push(
            FunderOutgoingComm::ChannelerConfig(channeler_config));
    }

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

