mod handle_control;
mod handle_friend;
mod handle_liveness;
mod handle_init;
mod sender;
mod canceler;

use futures::prelude::{async, await};

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::state::{FunderState, FunderMutation};
use self::handle_control::{HandleControlError};
use self::handle_friend::HandleFriendError;
use super::token_channel::directional::{ReceiveMoveTokenError, FriendMoveTokenRequest};
use super::types::{FriendMoveToken, FriendsRoute, 
    IncomingControlMessage, IncomingLivenessMessage, ChannelToken};
use super::ephemeral::FunderEphemeral;
use super::friend::{FriendState, InconsistencyStatus};

use super::messages::{FunderCommand, ResponseSendFundsResult};

// Approximate maximum size of a MOVE_TOKEN message.
// TODO: Where to put this constant? Do we have more like this one?
const MAX_MOVE_TOKEN_LENGTH: usize = 0x1000;

#[allow(unused)]
pub struct FriendInconsistencyError {
    reset_token: ChannelToken,
    balance_for_reset: i128,
}

#[allow(unused)]
pub enum FriendMessage {
    MoveTokenRequest(FriendMoveTokenRequest),
    InconsistencyError(FriendInconsistencyError),
}

pub struct ResponseReceived {
    pub request_id: Uid,
    pub result: ResponseSendFundsResult,
}

pub enum ChannelerConfig<A> {
    AddFriend((PublicKey, Option<A>)),
    RemoveFriend(PublicKey),
}

/// An incoming message to the Funder:
pub enum FunderMessage<A> {
    Init,
    Liveness(IncomingLivenessMessage),
    Control(IncomingControlMessage<A>),
    Friend((PublicKey, FriendMessage)),
}

#[allow(unused)]
pub enum FunderTask<A> {
    FriendMessage((PublicKey, FriendMessage)),
    ChannelerConfig(ChannelerConfig<A>),
    ResponseReceived(ResponseReceived),
    StateUpdate, // TODO
}

pub enum FunderHandlerError {
    HandleControlError(HandleControlError),
    HandleFriendError(HandleFriendError),
    HandleLivenessError(!),
}

pub struct FunderHandlerOutput<A> {
    ephemeral: FunderEphemeral,
    mutations: Vec<FunderMutation<A>>,
    tasks: Vec<FunderTask<A>>,
}

pub struct MutableFunderHandler<A:Clone,R> {
    state: FunderState<A>,
    pub ephemeral: FunderEphemeral,
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
    mutations: Vec<FunderMutation<A>>,
    funder_tasks: Vec<FunderTask<A>>,
}

impl<A:Clone,R> MutableFunderHandler<A,R> {
    pub fn state(&self) -> &FunderState<A> {
        &self.state
    }

    fn get_friend(&self, friend_public_key: &PublicKey) -> Option<&FriendState<A>> {
        self.state.get_friends().get(&friend_public_key)
    }

    pub fn done(self) -> FunderHandlerOutput<A> {
        FunderHandlerOutput {
            ephemeral: self.ephemeral,
            mutations: self.mutations,
            tasks: self.funder_tasks,
        }
    }

    /// Apply a mutation and also remember it.
    pub fn apply_mutation(&mut self, messenger_mutation: FunderMutation<A>) {
        self.state.mutate(&messenger_mutation);
        self.mutations.push(messenger_mutation);
    }

    pub fn add_task(&mut self, messenger_task: FunderTask<A>) {
        self.funder_tasks.push(messenger_task);
    }

    /// Find the originator of a pending local request.
    /// This should be a pending remote request at some other friend.
    /// Returns the public key of a friend. If we are the origin of this request, the function return None.
    ///
    /// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
    /// between request_id and (friend_public_key, friend).
    pub fn find_request_origin(&self, request_id: &Uid) -> Option<&PublicKey> {
        for (friend_public_key, friend) in self.state.get_friends() {
            if friend.directional
                .token_channel
                .state()
                .pending_requests
                .pending_remote_requests
                .contains_key(request_id) {
                    return Some(friend_public_key)
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
        match friend.inconsistency_status {
            InconsistencyStatus::Empty => true,
            InconsistencyStatus::Outgoing(_) |
            InconsistencyStatus::IncomingOutgoing(_) => false,
        }
    }

}

fn gen_mutable<A:Clone, R: SecureRandom>(security_module_client: SecurityModuleClient,
                       rng: Rc<R>,
                       funder_state: &FunderState<A>,
                       funder_ephemeral: &FunderEphemeral) -> MutableFunderHandler<A,R> {

    MutableFunderHandler {
        state: funder_state.clone(),
        ephemeral: funder_ephemeral.clone(),
        security_module_client,
        rng,
        mutations: Vec::new(),
        funder_tasks: Vec::new(),
    }
}

#[async]
pub fn funder_handle_message<A: Clone + 'static, R: SecureRandom + 'static>(
                      security_module_client: SecurityModuleClient,
                      rng: Rc<R>,
                      funder_state: FunderState<A>,
                      funder_ephemeral: FunderEphemeral,
                      funder_message: FunderMessage<A>) 
        -> Result<FunderHandlerOutput<A>, FunderHandlerError> {

    let mut mutable_handler = gen_mutable(security_module_client.clone(),
                                          rng.clone(),
                                          &funder_state,
                                          &funder_ephemeral);

    let state = funder_state.clone();
    let ephemeral = funder_ephemeral.clone();
    let mutable_handler = match funder_message {
        FunderMessage::Init =>  {
            mutable_handler.handle_init();
            mutable_handler
        },
        FunderMessage::Liveness(liveness_message) =>
            await!(mutable_handler
                .handle_liveness_message(liveness_message))
                .map_err(FunderHandlerError::HandleLivenessError)?,
        FunderMessage::Control(control_message) => 
            await!(mutable_handler
                .handle_control_message(control_message))
                .map_err(FunderHandlerError::HandleControlError)?,
        FunderMessage::Friend((origin_public_key, friend_message)) => 
            await!(mutable_handler
                .handle_friend_message(origin_public_key, friend_message))
                .map_err(FunderHandlerError::HandleFriendError)?,
    };
    Ok(mutable_handler.done())
}

