
use std::rc::Rc;
use futures::channel::mpsc;
use futures::{Stream, stream, Sink, SinkExt, StreamExt};
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
            let funder_message = match await!(incoming_messages.next()) {
                Some(funder_message) => funder_message,
                None => return Err(FunderError::IncomingMessagesClosed),
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
            let mut comm_stream = stream::iter::<_>(handler_output.outgoing_comms);
            await!(comm_sender.send_all(&mut comm_stream))
                .map_err(|_| FunderError::SendCommError)?;


            // Send outgoing control messages:
            let mut control_stream = stream::iter::<_>(handler_output.outgoing_control);
            await!(control_sender.send_all(&mut control_stream))
                .map_err(|_| FunderError::SendControlError)?;

        }
    }
}


