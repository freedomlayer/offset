pub mod handle_control;
pub mod handle_friend;

use futures::prelude::{async, await};

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::state::{FunderState, FunderMutation};
use self::handle_control::HandleControlError;
use self::handle_friend::{FriendInconsistencyError,
    HandleFriendError, IncomingFriendMessage};
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{FriendMoveToken, FriendsRoute};
use super::ephemeral::FunderEphemeral;
use super::friend::FriendState;

use super::messages::{FunderCommand, ResponseSendFundsResult};


#[allow(unused)]
pub enum FriendMessage {
    MoveToken(FriendMoveToken),
    InconsistencyError(FriendInconsistencyError),
}

pub struct ResponseReceived {
    pub request_id: Uid,
    pub result: ResponseSendFundsResult,
}


#[allow(unused)]
pub enum FunderTask {
    FriendMessage(FriendMessage),
    ResponseReceived(ResponseReceived),
    // StateUpdate(()),
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
    funder_tasks: Vec<FunderTask>,
}

impl<A:Clone,R> MutableFunderHandler<A,R> {
    pub fn state(&self) -> &FunderState<A> {
        &self.state
    }

    fn get_friend(&self, friend_public_key: &PublicKey) -> Option<&FriendState<A>> {
        self.state.get_friends().get(&friend_public_key)
    }

    pub fn done(self) -> (FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask>) {
        (self.ephemeral, self.mutations, self.funder_tasks)
    }

    /// Apply a mutation and also remember it.
    pub fn apply_mutation(&mut self, messenger_mutation: FunderMutation<A>) {
        self.state.mutate(&messenger_mutation);
        self.mutations.push(messenger_mutation);
    }

    pub fn add_task(&mut self, messenger_task: FunderTask) {
        self.funder_tasks.push(messenger_task);
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

    #[allow(unused)]
    fn simulate_handle_timer_tick<A>(&self)
            -> Result<(Vec<FunderMutation<A>>, Vec<FunderTask>), ()> {
        // TODO
        unreachable!();
    }

    /*
    #[allow(unused)]
    fn simulate_handle_control_message<A: Clone>(&self,
                                        messenger_state: &FunderState<A>,
                                        funder_ephemeral: FunderEphemeral,
                                        funder_command: FunderCommand<A>)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask>), HandlerError> {
        let mut mutable_handler = self.gen_mutable(messenger_state,
                                                   funder_ephemeral);
        mutable_handler
            .handle_control_message(funder_command)
            .map_err(HandlerError::HandleControlError)?;

        Ok(mutable_handler.done())
    }

    #[allow(unused)]
    #[async]
    fn simulate_handle_friend_message<A: Clone>(self, 
                                        messenger_state: FunderState<A>,
                                        funder_ephemeral: FunderEphemeral,
                                        remote_public_key: PublicKey,
                                        friend_message: IncomingFriendMessage)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask>), HandlerError> {

        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   funder_ephemeral);
        let mutable_handler = await!(mutable_handler
            .handle_friend_message(remote_public_key, friend_message))
            .map_err(HandlerError::HandleFriendError)?;

        Ok(mutable_handler.done())
    }

    */

}

