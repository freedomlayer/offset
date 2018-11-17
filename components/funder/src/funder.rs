
use futures::channel::mpsc;
use futures::{future, stream, SinkExt, StreamExt};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crypto::crypto_rand::CryptoRandom;
use identity::IdentityClient;

use crate::ephemeral::Ephemeral;
use crate::handler::{funder_handle_message};
use crate::types::{FunderIncoming,
                    FunderOutgoingControl, FunderOutgoingComm,
                    IncomingControlMessage, IncomingCommMessage};
use crate::database::{DbCore, DbRunner, DbRunnerError};


#[derive(Debug)]
enum FunderError {
    IncomingControlClosed,
    IncomingCommClosed,
    IncomingMessagesError,
    DbRunnerError(DbRunnerError),
    SendControlError,
    SendCommError,
}


#[derive(Debug, Clone)]
enum FunderEvent<A> {
    FunderIncoming(FunderIncoming<A>),
    IncomingControlClosed,
    IncomingCommClosed,
}

async fn inner_funder<A: Serialize + DeserializeOwned + Send + Sync + Clone + 'static, R: CryptoRandom + 'static>(
    identity_client: IdentityClient,
    rng: R,
    incoming_control: mpsc::Receiver<IncomingControlMessage<A>>,
    incoming_comm: mpsc::Receiver<IncomingCommMessage>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A>>,
    db_core: DbCore<A>,
    mut opt_event_sender: Option<mpsc::Sender<FunderEvent<A>>>) -> Result<(), FunderError> {

    // Transform error type:
    let mut comm_sender = comm_sender.sink_map_err(|_| ());
    let mut control_sender = control_sender.sink_map_err(|_| ());

    let funder_state = db_core.state().clone();
    let mut db_runner = DbRunner::new(db_core);
    let mut ephemeral = Ephemeral::new(&funder_state);
    let _ = funder_state;

    // Select over all possible events:
    let incoming_control = incoming_control
        .map(|incoming_control_msg| FunderEvent::FunderIncoming(FunderIncoming::Control(incoming_control_msg)))
        .chain(stream::once(future::ready(FunderEvent::IncomingControlClosed)));
    let incoming_comm = incoming_comm
        .map(|incoming_comm_msg| FunderEvent::FunderIncoming(FunderIncoming::Comm(incoming_comm_msg)))
        .chain(stream::once(future::ready(FunderEvent::IncomingCommClosed)));
    // Chain the Init message first:
    let mut incoming_messages = stream::once(future::ready(FunderEvent::FunderIncoming(FunderIncoming::Init)))
        .chain(incoming_control.select(incoming_comm));

    while let Some(funder_event) = await!(incoming_messages.next()) {
        // For testing:
        if let Some(ref mut event_sender) = opt_event_sender {
            await!(event_sender.send(funder_event.clone())).unwrap();
        }
        // Read one message from incoming messages:
        let funder_incoming = match funder_event {
            FunderEvent::IncomingControlClosed => return Err(FunderError::IncomingControlClosed),
            FunderEvent::IncomingCommClosed => return Err(FunderError::IncomingCommClosed),
            FunderEvent::FunderIncoming(funder_incoming) => funder_incoming,
        }; 

        // Process message:
        let res = await!(funder_handle_message(identity_client.clone(),
                              rng.clone(),
                              db_runner.state().clone(),
                              ephemeral.clone(),
                              funder_incoming));

        let handler_output = match res {
            Ok(handler_output) => handler_output,
            Err(handler_error) => {
                // Reporting a recoverable error:
                error!("Funder handler error: {:?}", handler_error);
                continue;
            },
        };

        // Send mutations to database:
        db_runner = await!(db_runner.mutate(handler_output.funder_mutations))
            .map_err(FunderError::DbRunnerError)?;
        
        // Apply ephemeral mutations to our ephemeral:
        for mutation in &handler_output.ephemeral_mutations {
            ephemeral.mutate(mutation);
        }

        // Send outgoing communication messages:
        let mut comm_stream = stream::iter::<_>(handler_output.outgoing_comms);
        await!(comm_sender.send_all(&mut comm_stream))
            .map_err(|_| FunderError::SendCommError)?;


        // Send outgoing control messages:
        let mut control_stream = stream::iter::<_>(handler_output.outgoing_control);
        await!(control_sender.send_all(&mut control_stream))
            .map_err(|_| FunderError::SendControlError)?;

    }
    // TODO: Do we ever really get here?
    Ok(())
}


#[allow(unused)]
pub async fn funder<A: Serialize + DeserializeOwned + Send + Sync + Clone + 'static, R: CryptoRandom + 'static>(
    identity_client: IdentityClient,
    rng: R,
    incoming_control: mpsc::Receiver<IncomingControlMessage<A>>,
    incoming_comm: mpsc::Receiver<IncomingCommMessage>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A>>,
    db_core: DbCore<A>) -> Result<(), FunderError> {

    await!(inner_funder(identity_client,
           rng,
           incoming_control,
           incoming_comm,
           control_sender,
           comm_sender,
           db_core,
           None))

}

