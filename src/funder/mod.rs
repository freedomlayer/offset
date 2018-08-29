#![allow(dead_code, unused)]

use std::rc::Rc;
use futures::prelude::{async, await};
use futures::{sync::mpsc, Stream};

use ring::rand::SecureRandom;

use self::state::FunderState;
use self::ephemeral::FunderEphemeral;
use self::handler::{funder_handle_message, 
    FunderHandlerOutput, FunderHandlerError};
use self::types::{FunderMessage, ResponseReceived};

use security_module::client::SecurityModuleClient;

pub mod messages;
// pub mod client;
mod liveness;
mod ephemeral;
mod credit_calc;
mod freeze_guard;
mod signature_buff; 
mod friend;
mod state;
mod types;
mod token_channel;
mod handler;
mod report;

enum FunderError {
    IncomingMessagesClosed,
    IncomingMessagesError,
}


struct Funder<A: Clone, R> {
    security_module_client: SecurityModuleClient,
    rng: Rc<R>,
    funder_state: FunderState<A>,
    funder_ephemeral: FunderEphemeral,
    incoming_messages: mpsc::Receiver<FunderMessage<A>>,
    outgoing_control: mpsc::Sender<()>,     // TODO
    outgoing_comm: mpsc::Sender<()>,        // TODO
}

enum OutgoingControl {
    ResponseReceived(ResponseReceived),
    StateUpdate, // TODO
}

impl<A: Clone + 'static, R: SecureRandom + 'static> Funder<A,R> {
    #[async]
    fn run(mut self) -> Result<!, FunderError> {

        let Funder {security_module_client,
                    rng,
                    funder_state,
                    funder_ephemeral,
                    mut incoming_messages,
                    .. } = self;

        loop {
            // Read one message from incoming messages:
            let funder_message = match await!(incoming_messages.into_future()) {
                Ok((opt_funder_message, ret_incoming_messages)) => {
                    incoming_messages = ret_incoming_messages;
                    match opt_funder_message {
                        Some(funder_message) => funder_message,
                        None => return Err(FunderError::IncomingMessagesClosed),
                    }
                },
                Err(_) => return Err(FunderError::IncomingMessagesError),
            };

            // Process message:
            let res = await!(funder_handle_message(security_module_client.clone(),
                                  rng.clone(),
                                  funder_state.clone(),
                                  funder_ephemeral.clone(),
                                  funder_message));

            let handle_output = match res {
                Ok(handler_output) => handler_output,
                Err(handler_error) => {
                    // Reporting a recoverable error:
                    error!("Funder handler error: {:?}", handler_error);
                    continue;
                },
            };
            // TODO; Handle output here:
            // - Send mutations to database.
            // - Send outgoing control messages:
            //      - ResponseReceived,
            //      - StateUpdate,
            // - Send outgoing communication messages:     
            //      - ChannelerConfig
            //      - FriendMessage
            unimplemented!();

        }
    }
}


