mod handle_control;
mod handle_friend;
mod handle_liveness;
mod handle_init;
mod sender;
mod canceler;

#[cfg(test)]
mod tests;

use identity::IdentityClient;

use crypto::uid::Uid;
use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use super::state::{FunderState, FunderMutation};
use self::handle_control::{HandleControlError};
use self::handle_friend::HandleFriendError;
use self::handle_liveness::HandleLivenessError;
use super::types::{FriendMoveTokenRequest, FunderIncoming,
    FunderOutgoingComm, FunderOutgoingControl, IncomingCommMessage,
    ResponseReceived};
use super::ephemeral::FunderEphemeral;
use super::friend::{FriendState, ChannelStatus};
use super::report::create_report;


// Approximate maximum size of a MOVE_TOKEN message.
// TODO: Where to put this constant? Do we have more like this one?
const MAX_MOVE_TOKEN_LENGTH: usize = 0x1000;


#[derive(Debug)]
pub enum FunderHandlerError {
    HandleControlError(HandleControlError),
    HandleFriendError(HandleFriendError),
    HandleLivenessError(HandleLivenessError),
}

pub struct FunderHandlerOutput<A: Clone> {
    pub ephemeral: FunderEphemeral,
    pub mutations: Vec<FunderMutation<A>>,
    pub outgoing_comms: Vec<FunderOutgoingComm<A>>,
    pub outgoing_control: Vec<FunderOutgoingControl<A>>,
}

pub struct MutableFunderHandler<A:Clone,R> {
    state: FunderState<A>,
    pub ephemeral: FunderEphemeral,
    pub identity_client: IdentityClient,
    pub rng: R, // Can we be more generic and remove this Rc?
    mutations: Vec<FunderMutation<A>>,
    outgoing_comms: Vec<FunderOutgoingComm<A>>,
    responses_received: Vec<ResponseReceived>,
}

impl<A:Clone + 'static,R> MutableFunderHandler<A,R> {
    /*
    pub fn state(&self) -> &FunderState<A> {
        &self.state
    }
    */

    fn get_friend(&self, friend_public_key: &PublicKey) -> Option<&FriendState<A>> {
        self.state.friends.get(&friend_public_key)
    }

    pub fn done(self) -> FunderHandlerOutput<A> {
        let mut outgoing_control = self.responses_received
            .into_iter()
            .map(FunderOutgoingControl::ResponseReceived)
            .collect::<Vec<FunderOutgoingControl<A>>>();

        // If anything is expected to change with the state, add a report to be sent through the
        // outgoing control channel:
        if !self.mutations.is_empty() {
            let funder_report = create_report(&self.state, &self.ephemeral.liveness);
            outgoing_control.push(FunderOutgoingControl::Report(funder_report));
        }

        FunderHandlerOutput {
            ephemeral: self.ephemeral,
            mutations: self.mutations,
            outgoing_comms: self.outgoing_comms,
            outgoing_control,
        }
    }

    /// Apply a mutation and also remember it.
    pub fn apply_mutation(&mut self, messenger_mutation: FunderMutation<A>) {
        self.state.mutate(&messenger_mutation);
        self.mutations.push(messenger_mutation);
    }

    pub fn add_outgoing_comm(&mut self, outgoing_comm: FunderOutgoingComm<A>) {
        self.outgoing_comms.push(outgoing_comm);
    }

    pub fn add_response_received(&mut self, response_received: ResponseReceived) {
        self.responses_received.push(response_received);
    }

    /// Find the originator of a pending local request.
    /// This should be a pending remote request at some other friend.
    /// Returns the public key of a friend. If we are the origin of this request, the function returns None.
    ///
    /// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
    /// between request_id and (friend_public_key, friend).
    pub fn find_request_origin(&self, request_id: &Uid) -> Option<&PublicKey> {
        for (friend_public_key, friend) in &self.state.friends {
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

    /// Is it a good idea to forward requests to this friend at this moment?
    /// This checks if it is likely that the friend will answer in a timely manner.
    pub fn is_friend_ready(&self, friend_public_key: &PublicKey) -> bool {
        let friend = self.get_friend(friend_public_key).unwrap();
        if !self.ephemeral.liveness.is_online(friend_public_key) {
            return false;
        }
        if let ChannelStatus::Inconsistent(_) = friend.channel_status {
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

}

fn gen_mutable<A:Clone, R: CryptoRandom>(identity_client: IdentityClient,
                       rng: R,
                       funder_state: &FunderState<A>,
                       funder_ephemeral: &FunderEphemeral) -> MutableFunderHandler<A,R> {

    MutableFunderHandler {
        state: funder_state.clone(),
        ephemeral: funder_ephemeral.clone(),
        identity_client,
        rng,
        mutations: Vec::new(),
        outgoing_comms: Vec::new(),
        responses_received: Vec::new(),
    }
}

pub async fn funder_handle_message<A: Clone + 'static, R: CryptoRandom + 'static>(
                      identity_client: IdentityClient,
                      rng: R,
                      funder_state: FunderState<A>,
                      funder_ephemeral: FunderEphemeral,
                      funder_incoming: FunderIncoming<A>) 
        -> Result<FunderHandlerOutput<A>, FunderHandlerError> {

    let mut mutable_handler = gen_mutable(identity_client.clone(),
                                          rng.clone(),
                                          &funder_state,
                                          &funder_ephemeral);

    match funder_incoming {
        FunderIncoming::Init =>  {
            mutable_handler.handle_init();
        },
        FunderIncoming::Control(control_message) =>
            await!(mutable_handler
                .handle_control_message(control_message))
                .map_err(FunderHandlerError::HandleControlError)?,
        FunderIncoming::Comm(incoming_comm) => {
            match incoming_comm {
                IncomingCommMessage::Liveness(liveness_message) =>
                    await!(mutable_handler
                        .handle_liveness_message(liveness_message))
                        .map_err(FunderHandlerError::HandleLivenessError)?,
                IncomingCommMessage::Friend((origin_public_key, friend_message)) => 
                    await!(mutable_handler
                        .handle_friend_message(origin_public_key, friend_message))
                        .map_err(FunderHandlerError::HandleFriendError)?,
            }
        },
    };
    Ok(mutable_handler.done())
}

