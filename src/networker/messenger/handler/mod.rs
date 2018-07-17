mod handle_app_manager;
pub mod handle_neighbor;
mod handle_funder;
mod handle_crypter;

use std::rc::Rc;
use security_module::client::SecurityModuleClient;
use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use super::messenger_state::{MessengerState, StateMutateMessage};
use self::handle_neighbor::{NeighborInconsistencyError, 
    NeighborSetMaxTokenChannels};
use super::token_channel::directional::ReceiveMoveTokenError;
use super::types::{NeighborMoveToken, NeighborsRoute};

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

#[allow(unused)]
pub struct MessengerHandler<R> {
    pub state: MessengerState,
    pub security_module_client: SecurityModuleClient,
    pub rng: Rc<R>,
    pub sm_messages: Vec<StateMutateMessage>,
    pub messenger_tasks: Vec<MessengerTask>,
}

impl<R: SecureRandom> MessengerHandler<R> {
    #[allow(unused)]
    pub fn handle_timer_tick(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }
}
