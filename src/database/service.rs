#![allow(unused)]

use std::marker::Send;
use futures::prelude::{async, await};
use futures::sync::{oneshot, mpsc};
use futures::Stream;
use futures_cpupool::CpuPool;

use serde::Serialize;
use serde::de::DeserializeOwned;

use funder::state::{FunderMutation};
use super::core::{DbCore, DbCoreError};


pub struct IncomingMutationsBatch<A> {
    funder_mutations: Vec<FunderMutation<A>>,
    /// A oneshot to respond that the mutation was applied and the new state was saved.
    ack_sender: oneshot::Sender<()>,
}

pub enum DbServiceError {
    /// Incoming mutations stream closed
    IncomingClosed,
    /// Some error occured when trying to read an incoming batch
    IncomingError,
    DbCoreError(DbCoreError),
    /// Error when trying to send an ack
    AckFailure,
}

fn apply_funder_mutations<A: Clone + Serialize + DeserializeOwned>(
    mut db_core: DbCore<A>, funder_mutations: Vec<FunderMutation<A>>) -> Result<DbCore<A>,DbCoreError> {

    db_core.mutate(funder_mutations)?;
    Ok(db_core)
}

#[async]
pub fn db_service<A: Clone + Serialize + DeserializeOwned + Send + Sync + 'static>(mut db_core: DbCore<A>, 
                mut incoming_batches: mpsc::Receiver<IncomingMutationsBatch<A>>) -> Result<!, DbServiceError> {

    // Start a pool to run slow database operations:
    let pool = CpuPool::new(1);

    loop {
        // Read one incoming batch of mutations
        let incoming_mutations_batch = match await!(incoming_batches.into_future()) {
            Ok((opt_incoming_mutations_batch, ret_incoming_batches)) => {
                incoming_batches = ret_incoming_batches;
                match opt_incoming_mutations_batch {
                    Some(incoming_mutations_batch) => incoming_mutations_batch,
                    None => return Err(DbServiceError::IncomingClosed),
                }
            },
            Err(_) => return Err(DbServiceError::IncomingError),
        };

        let IncomingMutationsBatch {funder_mutations, ack_sender} = incoming_mutations_batch;

        db_core = await!(pool.spawn_fn(move || apply_funder_mutations(db_core, funder_mutations)))
            .map_err(DbServiceError::DbCoreError)?;

        // Send an ack to signal that the operation has completed:
        ack_sender.send(())
            .map_err(|()| DbServiceError::AckFailure)?;
    }
}
