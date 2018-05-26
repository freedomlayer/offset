#![allow(dead_code)]

use std::rc::Rc;
use std::collections::HashMap;
use std::net::SocketAddr;

use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use ring::rand::SecureRandom;

use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use timer::messages::FromTimer;

use super::messages::{NetworkerToChanneler, NetworkerToDatabase, 
    NetworkerToAppManager, MessageReceived, MoveTokenDirection,
    NeighborStatus};

use super::crypter::messages::CrypterRequestSendMessage;

use app_manager::messages::AppManagerToNetworker;
use security_module::messages::ToSecurityModule;
use security_module::client::SecurityModuleClient;

use funder::client::FunderClient;
use funder::messages::RequestSendFunds;

use database::clients::networker_client::DBNetworkerClient;

use channeler::messages::ChannelerToNetworker;

use proto::funder::InvoiceId;
use proto::networker::{ChannelToken};


pub mod token_channel;
pub mod neighbor_tc_logic;
mod tc_credit;
mod pending_requests;
mod invoice_validator;
pub mod balance_state;
mod messenger_messages;
pub mod pending_neighbor_request;
pub mod credit_calc;

use self::pending_neighbor_request::PendingNeighborRequest;


/// Full state of a Neighbor token channel.
struct NeighborTokenChannel {
    pub move_token_direction: MoveTokenDirection,
    pub move_token_message: Vec<u8>,
    // Raw bytes of last incoming/outgoing Move Token Message.
    // We already processed this message.
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(move_token_message)
    pub remote_max_debt: u64,
    pub local_max_debt: u64,
    pub remote_pending_debt: u64,
    pub local_pending_debt: u64,
    pub balance: i64,
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
    pub pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pub pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

struct NeighborState {
    neighbor_socket_addr: Option<SocketAddr>, 
    wanted_remote_max_debt: u64,
    wanted_max_channels: u32,
    status: NeighborStatus,
    // Enabled or disabled?
    token_channels: HashMap<u32, NeighborTokenChannel>,
    ticks_since_last_incoming: usize,
    // Number of time ticks since last incoming message
    ticks_since_last_outgoing: usize,
    // Number of time ticks since last outgoing message
}


#[allow(too_many_arguments)]
pub fn create_messenger<SR: SecureRandom>(handle: &Handle,
                        secure_rng: Rc<SR>,
                        timer_receiver: mpsc::Receiver<FromTimer>,
                        channeler_sender: mpsc::Sender<NetworkerToChanneler>,
                        channeler_receiver: mpsc::Receiver<ChannelerToNetworker>,
                        database_sender: mpsc::Sender<NetworkerToDatabase>,
                        funder_sender: mpsc::Sender<RequestSendFunds>,
                        app_manager_sender: mpsc::Sender<NetworkerToAppManager>,
                        app_manager_receiver: mpsc::Receiver<AppManagerToNetworker>,
                        security_module_sender: mpsc::Sender<ToSecurityModule>,
                        crypter_sender: mpsc::Sender<MessageReceived>,
                        crypter_receiver: mpsc::Receiver<CrypterRequestSendMessage>) {

    let funder_client = FunderClient::new(funder_sender);
    let database_client = DBNetworkerClient::new(database_sender);
    let security_module_client = SecurityModuleClient::new(security_module_sender);

}
