#![allow(unused)]

use std::marker::Send;
use futures::channel::{oneshot, mpsc};
use futures::{future, FutureExt, Stream};
use futures::task::SpawnExt;
// use futures_cpupool::CpuPool;
use futures::executor::ThreadPool;

use serde::Serialize;
use serde::de::DeserializeOwned;

use identity::IdentityClient;

use crate::state::{FunderMutation, FunderState};
use super::core::{DbCore, DbCoreError};

/*

pub struct IncomingMutationsBatch<A> {
    pub funder_mutations: Vec<FunderMutation<A>>,
    /// A oneshot to respond that the mutation was applied and the new state was saved.
    pub ack_sender: oneshot::Sender<()>,
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
*/

fn apply_funder_mutations<A: Clone + Serialize + DeserializeOwned + 'static>(
    mut db_core: DbCore<A>, 
    funder_mutations: Vec<FunderMutation<A>>) -> Result<DbCore<A>,DbCoreError> {

    db_core.mutate(funder_mutations)?;
    Ok(db_core)
}

/*

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
*/

#[derive(Debug)]
pub enum DbRunnerError {
    DbCoreError(DbCoreError),
}

pub struct DbRunner<A: Clone> {
    pool: ThreadPool,
    db_core: DbCore<A>,
}

impl<A: Clone + Serialize + DeserializeOwned + Send + Sync + 'static> DbRunner<A> {
    pub fn new(db_core: DbCore<A>) -> DbRunner<A> {
        // Start a pool to run slow database operations:
        DbRunner {
            pool: ThreadPool::new().unwrap(),
            db_core,
        }
    }

    pub async fn mutate(self, funder_mutations: Vec<FunderMutation<A>>) -> Result<Self, DbRunnerError> {
        let DbRunner {mut pool, db_core} = self;
        let fut_apply_db_mutation = future::lazy(move |_| apply_funder_mutations(db_core, funder_mutations));
        let handle = pool.spawn_with_handle(fut_apply_db_mutation).unwrap();
        let db_core = await!(handle)
            .map_err(DbRunnerError::DbCoreError)?;
        Ok(DbRunner {pool, db_core})
    }

    pub fn state(&self) -> &FunderState<A> {
        self.db_core.state()
    }
}

