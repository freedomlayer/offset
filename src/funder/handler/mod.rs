pub mod handle_control;
pub mod handle_friend;
pub mod handle_timer;

use futures::prelude::{async, await};

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::{PublicKey, Signature};
use crypto::rand_values::RandValue;

use proto::funder::ChannelToken;

use super::state::{FunderState, FunderMutation};
use self::handle_control::{HandleControlError, IncomingControlMessage};
use self::handle_friend::HandleFriendError;
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{FriendMoveToken, FriendsRoute, 
    PendingFriendRequest, FailureSendFunds};
use super::ephemeral::FunderEphemeral;
use super::friend::FriendState;
use super::signature_buff::{create_failure_signature_buffer};

use super::messages::{FunderCommand, ResponseSendFundsResult};

#[allow(unused)]
pub struct FriendInconsistencyError {
    opt_ack: Option<ChannelToken>,
    current_token: ChannelToken,
    balance_for_reset: i128,
}

#[allow(unused)]
pub enum FriendMessage {
    MoveToken(FriendMoveToken),
    MoveTokenAck(ChannelToken), // acked_token
    RequestToken(ChannelToken), // last_token
    InconsistencyError(FriendInconsistencyError),
    KeepAlive,
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

#[derive(Debug)]
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

impl<A:Clone + 'static, R:SecureRandom + 'static> MutableFunderHandler<A,R> {
    pub fn state(&self) -> &FunderState<A> {
        &self.state
    }

    /// Create a (signed) failure message for a given request_id.
    /// We are the reporting_public_key for this failure message.
    #[async]
    pub fn create_failure_message(mut self, pending_local_request: PendingFriendRequest) 
        -> Result<(Self, FailureSendFunds), HandlerError> {

        let rand_nonce = RandValue::new(&*self.rng);
        let local_public_key = self.state.get_local_public_key().clone();

        let failure_send_funds = FailureSendFunds {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key.clone(),
            rand_nonce: rand_nonce.clone(),
            signature: Signature::zero(),
        };
        // TODO: Add default() implementation for Signature
        
        let mut failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_funds,
                                            &pending_local_request);

        let signature = await!(self.security_module_client.request_signature(failure_signature_buffer))
            .unwrap();

        Ok((self, FailureSendFunds {
            request_id: pending_local_request.request_id,
            reporting_public_key: local_public_key,
            rand_nonce,
            signature,
        }))
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

    pub fn has_outgoing_message(&self) -> bool {
        for task in &self.funder_tasks {
            if let FunderTask::FriendMessage(_) = task {
                return true;
            }
        }
        false
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

    #[allow(unused,type_complexity)]
    fn simulate_handle_control_message<A: Clone + 'static>(self,
                                        messenger_state: &FunderState<A>,
                                        funder_ephemeral: &FunderEphemeral,
                                        funder_command: IncomingControlMessage<A>)
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask>), HandlerError> {
        let mut mutable_handler = self.gen_mutable(messenger_state,
                                                   funder_ephemeral);
        mutable_handler
            .handle_control_message(funder_command)
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
            -> Result<(FunderEphemeral, Vec<FunderMutation<A>>, Vec<FunderTask>), HandlerError> {

        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   &funder_ephemeral);
        let mutable_handler = await!(mutable_handler
            .handle_friend_message(remote_public_key, friend_message))
            .map_err(HandlerError::HandleFriendError)?;

        Ok(mutable_handler.done())
    }
}

