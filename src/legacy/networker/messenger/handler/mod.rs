mod handle_app_manager;
pub mod handle_neighbor;
mod handle_funder;
mod handle_crypter;

use futures::prelude::{async, await};

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::state::{MessengerState, MessengerMutation};
use self::handle_app_manager::HandleAppManagerError;
use self::handle_neighbor::{NeighborInconsistencyError,
     NeighborSetMaxTokenChannels, HandleNeighborError, IncomingNeighborMessage};
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{NeighborMoveToken, NeighborsRoute};
use super::cache::MessengerCache;

use app_manager::messages::{NetworkerCommand};

#[allow(unused)]
pub enum AppManagerMessage {
    ReceiveMoveTokenError(ReceiveMoveTokenError),
}

pub enum FunderMessage {

}


#[allow(unused)]
pub enum NeighborMessage {
    MoveToken(NeighborMoveToken),
    InconsistencyError(NeighborInconsistencyError),
    SetMaxTokenChannels(NeighborSetMaxTokenChannels),
}

pub struct RequestReceived {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content: Vec<u8>,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
}

pub struct ResponseReceived {
    pub request_id: Uid,
    pub processing_fee_collected: u64,
    pub response_content: Vec<u8>,
}

#[allow(unused)]
pub struct FailureReceived {
    pub request_id: Uid,
    pub reporting_public_key: PublicKey,
}


#[allow(unused)]
pub enum CrypterMessage {
    RequestReceived(RequestReceived),
    ResponseReceived(ResponseReceived),
    FailureReceived(FailureReceived),
}

/// Used for rebalancing a token channel by sending a payment to neighbor
/// along a route of friends.
#[allow(unused)]
pub struct SendPayment {
    neighbor_public_key: PublicKey,
    channel_index: u16,
    payment_id: Uid,
    payment: u64,   // Amount of credits to pay
}


#[allow(unused)]
pub enum MessengerTask {
    AppManagerMessage(AppManagerMessage),
    SendPayment(SendPayment),
    FunderMessage(FunderMessage),
    NeighborMessage(NeighborMessage),
    CrypterMessage(CrypterMessage),
}

pub enum HandlerError {
    HandleAppManagerError(HandleAppManagerError),
    HandleNeighborError(HandleNeighborError),
}

pub struct MutableMessengerHandler<R> {
    state: MessengerState,
    pub cache: MessengerCache,
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
    mutations: Vec<MessengerMutation>,
    messenger_tasks: Vec<MessengerTask>,
}

impl<R> MutableMessengerHandler<R> {
    pub fn state(&self) -> &MessengerState {
        &self.state
    }

    pub fn done(self) -> (MessengerCache, Vec<MessengerMutation>, Vec<MessengerTask>) {
        (self.cache, self.mutations, self.messenger_tasks)
    }

    /// Apply a mutation and also remember it.
    pub fn apply_mutation(&mut self, messenger_mutation: MessengerMutation) {
        self.state.mutate(&messenger_mutation);
        self.mutations.push(messenger_mutation);
    }

    pub fn add_task(&mut self, messenger_task: MessengerTask) {
        self.messenger_tasks.push(messenger_task);
    }
}


pub struct MessengerHandler<R> {
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
}

impl<R: SecureRandom + 'static> MessengerHandler<R> {

    fn gen_mutable(&self, messenger_state: &MessengerState,
                   messenger_cache: MessengerCache) -> MutableMessengerHandler<R> {
        MutableMessengerHandler {
            state: messenger_state.clone(),
            cache: messenger_cache,
            security_module_client: self.security_module_client.clone(),
            rng: self.rng.clone(),
            mutations: Vec::new(),
            messenger_tasks: Vec::new(),
        }
    }

    #[allow(unused)]
    fn simulate_handle_timer_tick(&self)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        // TODO
        unreachable!();
    }

    #[allow(unused)]
    fn simulate_handle_app_manager_message(&self,
                                        messenger_state: &MessengerState,
                                        messenger_cache: MessengerCache,
                                        networker_command: NetworkerCommand)
            -> Result<(MessengerCache, Vec<MessengerMutation>, Vec<MessengerTask>), HandlerError> {
        let mut mutable_handler = self.gen_mutable(messenger_state,
                                                   messenger_cache);
        mutable_handler
            .handle_app_manager_message(networker_command)
            .map_err(HandlerError::HandleAppManagerError)?;

        Ok(mutable_handler.done())
    }

    #[allow(unused)]
    #[async]
    fn simulate_handle_neighbor_message(self, 
                                        messenger_state: MessengerState,
                                        messenger_cache: MessengerCache,
                                        remote_public_key: PublicKey,
                                        neighbor_message: IncomingNeighborMessage)
            -> Result<(MessengerCache, Vec<MessengerMutation>, Vec<MessengerTask>), HandlerError> {

        let mut mutable_handler = self.gen_mutable(&messenger_state,
                                                   messenger_cache);
        let mutable_handler = await!(mutable_handler
            .handle_neighbor_message(remote_public_key, neighbor_message))
            .map_err(HandlerError::HandleNeighborError)?;

        Ok(mutable_handler.done())
    }

    #[allow(unused)]
    fn simulate_handle_funder_message(&self, 
                                        messenger_state: &MessengerState)
            -> Result<(MessengerCache, Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        unreachable!();
    }

    #[allow(unused)]
    fn simulate_handle_crypter_message(&self, 
                                        messenger_state: &MessengerState)
            -> Result<(MessengerCache, Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        unreachable!();
    }

}

