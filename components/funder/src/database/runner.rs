// use std::marker::Send;
use futures::{future};
use futures::task::SpawnExt;
// use futures_cpupool::CpuPool;
use futures::executor::ThreadPool;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::state::{FunderMutation, FunderState};
use super::atomic_db::AtomicDb;

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

fn apply_funder_mutations<A,D,E>(mut atomic_db: D, 
    funder_mutations: Vec<FunderMutation<A>>) -> Result<D, D::Error> 
where
    A: Clone + Serialize + DeserializeOwned + 'static,
    D: AtomicDb<State=FunderState<A>, Mutation=FunderMutation<A>, Error=E>,
{
    atomic_db.mutate(funder_mutations)?;
    Ok(atomic_db)
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
pub enum DbRunnerError<E> {
    AtomicDbError(E),
}

pub struct DbRunner<D> {
    pool: ThreadPool,
    opt_atomic_db: Option<D>,
}

impl<A,D,E> DbRunner<D> 
where
    A: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    D: AtomicDb<State=FunderState<A>, Mutation=FunderMutation<A>, Error=E> + Send + 'static,
    E: Send + 'static,
{
    pub fn new(atomic_db: D) -> DbRunner<D> {
        // Start a pool to run slow database operations:
        DbRunner {
            pool: ThreadPool::new().unwrap(),
            opt_atomic_db: Some(atomic_db),
        }
    }

    pub async fn mutate(&mut self, funder_mutations: Vec<FunderMutation<A>>) -> Result<(), DbRunnerError<E>> {
        let atomic_db = match self.opt_atomic_db.take() {
            None => unreachable!(),
            Some(atomic_db) => atomic_db
        };
        let fut_apply_db_mutation = future::lazy(move |_| apply_funder_mutations::<_,_,E>(atomic_db, funder_mutations));
        let handle = self.pool.spawn_with_handle(fut_apply_db_mutation).unwrap();
        let atomic_db = await!(handle)
            .map_err(DbRunnerError::AtomicDbError)?;
        self.opt_atomic_db = Some(atomic_db);
        Ok(())
    }

    pub fn get_state(&self) -> &FunderState<A> {
        match &self.opt_atomic_db {
            Some(atomic_db) => atomic_db.get_state(),
            None => unreachable!(),
        }
    }
}

