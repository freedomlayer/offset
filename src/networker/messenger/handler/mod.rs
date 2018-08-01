mod handle_app_manager;
pub mod handle_neighbor;
mod handle_funder;
mod handle_crypter;

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::messenger_state::{MessengerState, MessengerMutation};
use self::handle_app_manager::HandleAppManagerError;
use self::handle_neighbor::{NeighborInconsistencyError, 
     NeighborSetMaxTokenChannels};
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{NeighborMoveToken, NeighborsRoute};

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


#[allow(unused)]
pub enum MessengerTask {
    AppManagerMessage(AppManagerMessage),
    FunderMessage(FunderMessage),
    NeighborMessage(NeighborMessage),
    CrypterMessage(CrypterMessage),
}

pub enum HandlerError {
    HandleAppManagerError(HandleAppManagerError),
}

pub struct MutableMessengerHandler<R> {
    state: MessengerState,
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
    pub sm_messages: Vec<MessengerMutation>,
    pub messenger_tasks: Vec<MessengerTask>,
}

impl<R> MutableMessengerHandler<R> {
    pub fn state(&self) -> &MessengerState {
        &self.state
    }
}

pub struct MessengerHandler<R> {
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
}

impl<R: SecureRandom> MessengerHandler<R> {

    fn gen_mutable(&self, messenger_state: &MessengerState) -> MutableMessengerHandler<R> {
        MutableMessengerHandler {
            state: messenger_state.clone(),
            security_module_client: self.security_module_client.clone(),
            rng: self.rng.clone(),
            sm_messages: Vec::new(),
            messenger_tasks: Vec::new(),
        }
    }

    #[allow(unused)]
    fn simulate_handle_timer_tick(&mut self)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        // TODO
        unreachable!();
    }

    #[allow(unused)]
    fn simulate_handle_app_manager_message(&self,
                                        messenger_state: &MessengerState,
                                        networker_command: NetworkerCommand)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), HandlerError> {
        let mut mutable_handler = self.gen_mutable(messenger_state);
        mutable_handler
            .handle_app_manager_message(networker_command)
            .map_err(HandlerError::HandleAppManagerError)
    }

    #[allow(unused)]
    fn simulate_handle_neighbor_message(&self, 
                                        messenger_state: &MessengerState)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        unreachable!();
    }

    #[allow(unused)]
    fn simulate_handle_funder_message(&self, 
                                        messenger_state: &MessengerState)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        unreachable!();
    }

    #[allow(unused)]
    fn simulate_handle_crypter_message(&self, 
                                        messenger_state: &MessengerState)
            -> Result<(Vec<MessengerMutation>, Vec<MessengerTask>), ()> {
        unreachable!();
    }

}

