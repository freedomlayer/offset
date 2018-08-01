#![warn(unused)]

use std::rc::Rc;

use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use ring::rand::SecureRandom;

use timer::messages::FromTimer;

use super::messages::{NetworkerToChanneler, NetworkerToDatabase, 
    NetworkerToAppManager, MessageReceived};

use super::crypter::messages::CrypterRequestSendMessage;

use app_manager::messages::AppManagerToNetworker;
use security_module::messages::ToSecurityModule;
use security_module::client::SecurityModuleClient;

use funder::client::FunderClient;
use funder::messages::RequestSendFunds;

use database::clients::networker_client::DBNetworkerClient;

use channeler::messages::ChannelerToNetworker;

pub mod token_channel;
mod slot;
mod neighbor;
mod handler;
pub mod types;
mod credit_calc;
mod signature_buff;
mod state;
mod cache;


#[allow(unused)]
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
