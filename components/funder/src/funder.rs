
use std::rc::Rc;
use futures::channel::mpsc;
use futures::{Stream, stream, Sink};
use futures_cpupool::CpuPool;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crypto::crypto_rand::CryptoRandom;
use identity::IdentityClient;

use crate::state::FunderState;
use crate::ephemeral::FunderEphemeral;
use crate::handler::{funder_handle_message, 
    FunderHandlerOutput, FunderHandlerError};
use crate::types::{FunderOutgoing, FunderIncoming, ResponseReceived,
                    FunderOutgoingControl, FunderOutgoingComm};
use crate::database::{DbCore, DbRunner, DbRunnerError};



enum FunderError {
    IncomingMessagesClosed,
    IncomingMessagesError,
    DbRunnerError(DbRunnerError),
    SendControlError,
    SendCommError,
}


struct Funder<A: Clone, R> {
    identity_client: IdentityClient,
    rng: Rc<R>,
    incoming_messages: mpsc::Receiver<FunderIncoming<A>>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A>>,
    db_core: DbCore<A>,
}

impl<A: Serialize + DeserializeOwned + Send + Sync + Clone + 'static, R: CryptoRandom + 'static> Funder<A,R> {
    async fn run(mut self) -> Result<!, FunderError> {

        let Funder {identity_client,
                    rng,
                    mut incoming_messages,
                    control_sender,
                    comm_sender,
                    db_core} = self;


        // Transform error type:
        let mut comm_sender = comm_sender.sink_map_err(|_| ());
        let mut control_sender = control_sender.sink_map_err(|_| ());

        let funder_state = db_core.state().clone();
        let mut db_runner = DbRunner::new(db_core);
        let mut funder_ephemeral = FunderEphemeral::new(&funder_state);
        let _ = funder_state;

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
            let res = await!(funder_handle_message(identity_client.clone(),
                                  rng.clone(),
                                  db_runner.state().clone(),
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
            
            // Keep new funder_ephemeral:
            funder_ephemeral = handler_output.ephemeral;

            // Send outgoing communication messages:
            let comm_stream = stream::iter::<_>(handler_output.outgoing_comms);
            let (ret_comm_sender, _) = await!(comm_sender.send_all(comm_stream))
                .map_err(|_| FunderError::SendCommError)?;
            comm_sender = ret_comm_sender;


            // Send outgoing control messages:
            let control_stream = stream::iter::<_>(handler_output.outgoing_control);
            let (ret_control_sender, _) = await!(control_sender.send_all(control_stream))
                .map_err(|_| FunderError::SendControlError)?;
            control_sender = ret_control_sender;

        }
    }
}


