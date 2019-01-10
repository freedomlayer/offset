use std::fmt::Debug;
use std::hash::Hash;

use futures::channel::mpsc;
use futures::{future, stream, SinkExt, StreamExt};

use serde::Serialize;
use serde::de::DeserializeOwned;

use identity::IdentityClient;

use common::canonical_serialize::CanonicalSerialize;
use proto::funder::messages::{FunderIncomingControl, 
    FunderOutgoingControl};

use crate::ephemeral::Ephemeral;
use crate::handler::{funder_handle_message};
use crate::types::{FunderIncoming, FunderOutgoingComm, FunderIncomingComm};
use crate::state::{FunderState, FunderMutation};
use crate::database::{AtomicDb, DbRunner, DbRunnerError};

use crate::sign_verify::{SignMoveToken, SignResponse, SignFailure,
                            GenRandToken, GenRandNonce};


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
pub enum FunderEvent<A,P,RS,FS,MS> {
    FunderIncoming(FunderIncoming<A,P,RS,FS,MS>),
    IncomingControlClosed,
    IncomingCommClosed,
}

pub async fn inner_funder_loop<A,P,RS,FS,MS,SI,R,D,E>(
    mut signer: SI,
    rng: R,
    incoming_control: mpsc::Receiver<FunderIncomingControl<A,P,RS,MS>>,
    incoming_comm: mpsc::Receiver<FunderIncomingComm<A,P,RS,FS,MS>>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A,P,RS,MS>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A,P,RS,FS,MS>>,
    atomic_db: D,
    max_operations_in_batch: usize,
    mut opt_event_sender: Option<mpsc::Sender<FunderEvent<A,P,RS,FS,MS>>>) -> Result<(), FunderError<E>> 

where
    A: CanonicalSerialize + Serialize + DeserializeOwned + Send + Sync + Clone + Debug + PartialEq + Eq + 'static,
    P: CanonicalSerialize + Hash + Eq + Clone + Send + Sync + Debug,
    RS: Eq + Clone + Send + Sync + Debug,
    FS: Eq + Clone + Send + Sync + Debug, 
    MS: Eq + Clone + Send + Sync + Debug,
    SI: SignMoveToken<A,P,RS,FS,MS> + SignResponse<P,RS> + SignFailure<P,FS>,
    R: GenRandToken<MS> + GenRandNonce + 'static,
    D: AtomicDb<State=FunderState<A,P,RS,FS,MS>, Mutation=FunderMutation<A,P,RS,FS,MS>, Error=E> + Send + 'static,
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
        let res = await!(funder_handle_message(&mut signer, 
                              &mut rng,
                              db_runner.get_state().clone(),
                              ephemeral.clone(),
                              max_operations_in_batch,
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
pub async fn funder_loop<A,P,RS,FS,MS,SI,R,D,E>(
    signer: SI,
    rng: R,
    incoming_control: mpsc::Receiver<FunderIncomingControl<A,P,RS,MS>>,
    incoming_comm: mpsc::Receiver<FunderIncomingComm<A,P,RS,FS,MS>>,
    control_sender: mpsc::Sender<FunderOutgoingControl<A,P,RS,MS>>,
    comm_sender: mpsc::Sender<FunderOutgoingComm<A,P,RS,FS,MS>>,
    max_operations_in_batch: usize,
    atomic_db: D) -> Result<(), FunderError<E>> 
where
    A: CanonicalSerialize + Serialize + DeserializeOwned + Send + Sync + Clone + Debug + PartialEq + Eq + 'static,
    P: CanonicalSerialize + Hash + Eq + Clone + Send + Sync + Debug,
    RS: Eq + Clone + Send + Sync + Debug,
    FS: Eq + Clone + Send + Sync + Debug, 
    MS: Eq + Clone + Send + Sync + Debug,
    SI: SignMoveToken<A,P,RS,FS,MS> + SignResponse<P,RS> + SignFailure<P,FS>,
    R: GenRandToken<MS> + GenRandNonce + 'static,
    D: AtomicDb<State=FunderState<A,P,RS,FS,MS>, Mutation=FunderMutation<A,P,RS,FS,MS>, Error=E> + Send + 'static,
    E: Send + 'static,
{

    await!(inner_funder_loop(signer,
           rng,
           incoming_control,
           incoming_comm,
           control_sender,
           comm_sender,
           atomic_db,
           max_operations_in_batch,
           None))

}


