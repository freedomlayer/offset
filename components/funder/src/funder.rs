use std::fmt::Debug;

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
                    IncomingControlMessage, FunderIncomingComm};
use crate::state::{FunderState, FunderMutation};
use crate::database::{AtomicDb, DbRunner, DbRunnerError};


#[derive(Debug)]
pub enum FunderError<E> {
    IncomingControlClosed,
    IncomingCommClosed,
    IncomingMessagesError,
    DbRunnerError(DbRunnerError<E>),
    SendControlError,
    SendCommError,
}


#[derive(Debug, Clone)]
pub enum FunderEvent<A> {
    FunderIncoming(FunderIncoming<A>),
    IncomingControlClosed,
    IncomingCommClosed,
}

pub async fn inner_funder_loop<A, R, D, E>(
    identity_client: IdentityClient,
    rng: R,
    incoming_control: mpsc::Receiver<IncomingControlMessage<A>>,
    incoming_comm: mpsc::Receiver<FunderIncomingComm>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A>>,
    atomic_db: D,
    mut opt_event_sender: Option<mpsc::Sender<FunderEvent<A>>>) -> Result<(), FunderError<E>> 

where
    A: Serialize + DeserializeOwned + Send + Sync + Clone + Debug + 'static,
    R: CryptoRandom + 'static,
    D: AtomicDb<State=FunderState<A>, Mutation=FunderMutation<A>, Error=E> + Send + 'static,
    E: Send + 'static,
{

    // Transform error type:
    let mut comm_sender = comm_sender.sink_map_err(|_| ());
    let mut control_sender = control_sender.sink_map_err(|_| ());

    let funder_state = atomic_db.get_state().clone();
    let mut db_runner = DbRunner::new(atomic_db);
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
        // Read one message from incoming messages:
        let funder_incoming = match funder_event.clone() {
            FunderEvent::IncomingControlClosed => return Err(FunderError::IncomingControlClosed),
            FunderEvent::IncomingCommClosed => return Err(FunderError::IncomingCommClosed),
            FunderEvent::FunderIncoming(funder_incoming) => funder_incoming,
        }; 

        // Process message:
        let res = await!(funder_handle_message(identity_client.clone(),
                              rng.clone(),
                              db_runner.get_state().clone(),
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

        if !handler_output.funder_mutations.is_empty() {
            // If there are any mutations, send them to the database:
            await!(db_runner.mutate(handler_output.funder_mutations))
                .map_err(FunderError::DbRunnerError)?;
        }
        
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


        if let Some(ref mut event_sender) = opt_event_sender {
            await!(event_sender.send(funder_event)).unwrap();
        }

    }
    // TODO: Do we ever really get here?
    Ok(())
}


#[allow(unused)]
pub async fn funder_loop<A,R,D,E>(
    identity_client: IdentityClient,
    rng: R,
    incoming_control: mpsc::Receiver<IncomingControlMessage<A>>,
    incoming_comm: mpsc::Receiver<FunderIncomingComm>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A>>,
    atomic_db: D) -> Result<(), FunderError<E>> 
where
    A: Serialize + DeserializeOwned + Send + Sync + Clone + Debug + 'static,
    R: CryptoRandom + 'static,
    D: AtomicDb<State=FunderState<A>, Mutation=FunderMutation<A>, Error=E> + Send + 'static,
    E: Send + 'static,
{

    await!(inner_funder_loop(identity_client,
           rng,
           incoming_control,
           incoming_comm,
           control_sender,
           comm_sender,
           atomic_db,
           None))

}


