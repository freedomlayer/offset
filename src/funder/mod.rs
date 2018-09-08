#![allow(dead_code, unused)]

use std::rc::Rc;
use futures::prelude::{async, await};
use futures::{sync::mpsc, Stream};
use futures_cpupool::CpuPool;

use serde::Serialize;
use serde::de::DeserializeOwned;

use ring::rand::SecureRandom;

use self::state::FunderState;
use self::ephemeral::FunderEphemeral;
use self::handler::{funder_handle_message, 
    FunderHandlerOutput, FunderHandlerError};
use self::types::{FunderOutgoing, FunderIncoming, ResponseReceived,
                    FunderOutgoingControl, FunderOutgoingComm};
use self::database::{DbCore, DbRunner, DbRunnerError};

use security_module::client::SecurityModuleClient;

pub mod messages;
// pub mod client;
mod liveness;
mod ephemeral;
mod credit_calc;
mod freeze_guard;
mod signature_buff; 
mod friend;
pub mod state;
mod types;
mod token_channel;
mod handler;
mod report;
mod database;

enum FunderError {
    IncomingMessagesClosed,
    IncomingMessagesError,
    DbRunnerError(DbRunnerError),
}


struct Funder<A: Clone, R> {
    security_module_client: SecurityModuleClient,
    rng: Rc<R>,
    funder_ephemeral: FunderEphemeral,
    incoming_messages: mpsc::Receiver<FunderIncoming<A>>,
    outgoing_control: mpsc::Sender<FunderOutgoingControl<A>>,
    outgoing_comm: mpsc::Sender<FunderOutgoingComm<A>>,
    db_core: DbCore<A>,
}

impl<A: Serialize + DeserializeOwned + Send + Sync + Clone + 'static, R: SecureRandom + 'static> Funder<A,R> {
    #[async]
    fn run(mut self) -> Result<!, FunderError> {

        let Funder {security_module_client,
                    rng,
                    funder_ephemeral,
                    mut incoming_messages,
                    outgoing_control,
                    outgoing_comm,
                    db_core} = self;

        let funder_state = db_core.state().clone();
        let mut db_runner = DbRunner::new(db_core);

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

            let handler_output = match res {
                Ok(handler_output) => handler_output,
                Err(handler_error) => {
                    // Reporting a recoverable error:
                    error!("Funder handler error: {:?}", handler_error);
                    continue;
                },
            };

            // Send mutations to database:
            db_runner = await!(db_runner.mutate(handler_output.mutations))
                .map_err(FunderError::DbRunnerError)?;
            

            // - Send outgoing communication messages:     
            //      - ChannelerConfig
            //      - FriendMessage
            for outgoing_comm in handler_output.outgoing_comms {
                unimplemented!();
            }

            // - Send outgoing control messages:
            //      - ResponseReceived,
            //      - StateUpdate,
            for outgoing_control in handler_output.outgoing_control {
                unimplemented!();
            }

            // Send a Report message through the outgoing control:

            unimplemented!();

        }
    }
}


