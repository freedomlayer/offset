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

use proto::funder::ChannelToken;

use super::state::{FunderState, FunderMutation};
use self::handle_control::{HandleControlError};
use self::handle_friend::HandleFriendError;
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{FriendMoveToken, FriendsRoute, 
    IncomingControlMessage, IncomingLivenessMessage};
use super::ephemeral::FunderEphemeral;
use super::friend::FriendState;

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
    MoveToken(FriendMoveToken),
    RequestToken(ChannelToken), // last_token
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


#[allow(unused)]
pub enum FunderTask<A> {
    FriendMessage((PublicKey, FriendMessage)),
    ChannelerConfig(ChannelerConfig<A>),
    ResponseReceived(ResponseReceived),
    StateUpdate, // TODO
}

pub enum HandlerError {
    HandleControlError(HandleControlError),
    HandleFriendError(HandleFriendError),
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

    pub fn done(self) -> (FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask<A>>) {
        (self.ephemeral, self.mutations, self.funder_tasks)
    }

    /// Apply a mutation and also remember it.
    pub fn apply_mutation(&mut self, messenger_mutation: FunderMutation<A>) {
        self.state.mutate(&messenger_mutation);
        self.mutations.push(messenger_mutation);
    }

    pub fn add_task(&mut self, messenger_task: FunderTask<A>) {
        self.funder_tasks.push(messenger_task);
    }

    pub fn has_outgoing_message(&self) -> bool {
        for task in &self.funder_tasks {
            if let FunderTask::FriendMessage(_) = task {
                return true;
            }
        }
        false
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
}


pub struct FunderHandler<R> {
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
}

impl<R: SecureRandom + 'static> FunderHandler<R> {

    fn gen_mutable<A:Clone>(&self, messenger_state: &FunderState<A>,
                   funder_ephemeral: &FunderEphemeral) -> MutableFunderHandler<A,R> {
        MutableFunderHandler {
            state: messenger_state.clone(),
            ephemeral: funder_ephemeral.clone(),
            security_module_client: self.security_module_client.clone(),
            rng: self.rng.clone(),
            mutations: Vec::new(),
            funder_tasks: Vec::new(),
        }
    }

    #[allow(unused, type_complexity)]
    #[async]
    fn simulate_handle_liveness_message<A: Clone + 'static>(self, 
                                        messenger_state: FunderState<A>,
                                        funder_ephemeral: FunderEphemeral,
                                        liveness_message: IncomingLivenessMessage)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask<A>>), HandlerError> {

        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   &funder_ephemeral);
        let mutable_handler = await!(mutable_handler
            .handle_liveness_message(liveness_message))
            .map_err(HandlerError::HandleFriendError)?;

        Ok(mutable_handler.done())
    }

    #[allow(unused,type_complexity)]
    #[async]
    fn simulate_handle_control_message<A: Clone + 'static>(self,
                                        messenger_state: FunderState<A>,
                                        funder_ephemeral: FunderEphemeral,
                                        funder_command: IncomingControlMessage<A>)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask<A>>), HandlerError> {
        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   &funder_ephemeral);
        let mutable_handler = await!(mutable_handler
            .handle_control_message(funder_command))
            .map_err(HandlerError::HandleControlError)?;

        Ok(mutable_handler.done())
    }

    #[allow(unused, type_complexity)]
    #[async]
    fn simulate_handle_friend_message<A: Clone + 'static>(self, 
                                        messenger_state: FunderState<A>,
                                        funder_ephemeral: FunderEphemeral,
                                        remote_public_key: PublicKey,
                                        friend_message: FriendMessage)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask<A>>), HandlerError> {

        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   &funder_ephemeral);
        let mutable_handler = await!(mutable_handler
            .handle_friend_message(remote_public_key, friend_message))
            .map_err(HandlerError::HandleFriendError)?;

        Ok(mutable_handler.done())
    }
}

