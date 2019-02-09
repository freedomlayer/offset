use futures::channel::{oneshot, mpsc};
use futures::{future, StreamExt, SinkExt};
use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use crate::atomic_db::AtomicDb;

#[derive(Debug)]
pub enum DatabaseError<ADE> {
    AtomicDbError(ADE),
    SpawnError,
    CreateThreadPoolError,
}

// A request to apply mutations to the database
pub struct DatabaseRequest<M> {
    pub mutations: Vec<M>,
    pub response_sender: oneshot::Sender<()>,
}

#[derive(Clone)]
pub struct DatabaseClient<M> {
    request_sender: mpsc::Sender<DatabaseRequest<M>>,
}

#[derive(Debug)]
pub enum DatabaseClientError {
    SendError,
    ResponseCanceled,
}

impl<M> DatabaseClient<M> {
    pub fn new(request_sender: mpsc::Sender<DatabaseRequest<M>>) -> Self {
        DatabaseClient {
            request_sender,
        }
    }

    pub async fn mutate(&mut self, mutations: Vec<M>) -> Result<(), DatabaseClientError> {

        let (response_sender, request_done) = oneshot::channel();
        let database_request = DatabaseRequest {
            mutations,
            response_sender,
        };
        // Send the request:
        await!(self.request_sender.send(database_request))
            .map_err(|_| DatabaseClientError::SendError)?;

        // Wait for ack from the service:
        await!(request_done)
            .map_err(|_| DatabaseClientError::ResponseCanceled)?;

        Ok(())
    }
}

pub async fn database_loop<AD>(mut atomic_db: AD, 
                               mut incoming_requests: mpsc::Receiver<DatabaseRequest<AD::Mutation>>)
                                -> Result<AD, DatabaseError<AD::Error>>
where
    AD: AtomicDb + Send + 'static,
    AD::Mutation: Send + 'static,
    AD::Error: Send + 'static,
{
    // We use an independent thread pool, to make sure our synchronous interaction
    // with the database doesn't block the rest of the main thread loop.
    // TODO: Maybe there will be a better way to do this in the future (Possibly a future version
    // of Tokio that has this feature)
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| DatabaseError::CreateThreadPoolError)?;

    while let Some(database_request) = await!(incoming_requests.next()) {
        let DatabaseRequest {mutations, response_sender} = database_request;
        let mutate_fut = future::lazy(move |_| {
            atomic_db.mutate_db(&mutations[..])
                .map_err(|atomic_db_error| DatabaseError::AtomicDbError(atomic_db_error))?;
            Ok(atomic_db)
        });
        let handle = thread_pool.spawn_with_handle(mutate_fut)
            .map_err(|_| DatabaseError::SpawnError)?;

        atomic_db = await!(handle)?;

        // Notify client that the database mutation request was processed:
        let _ = response_sender.send(());
    }
    // Return the current state
    Ok(atomic_db)
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};

    /// A dummy state (used for testing)
    #[derive(Debug)]
    struct DummyState {
        pub x: u32,
    }

    impl DummyState {
        pub fn new() -> Self {
            DummyState {
                x: 0u32,
            }
        }
    }

    /// A dummy mutation (used for testing)
    #[derive(Debug)]
    enum DummyMutation {
        Inc,
        Dec
    }

    /// A dummy AtomicDb (used for testing)
    #[derive(Debug)]
    struct DummyAtomicDb {
        pub dummy_state: DummyState,
    }

    impl DummyAtomicDb {
        pub fn new() -> Self {
            DummyAtomicDb {
                dummy_state: DummyState::new(),
            }
        }
    }

    impl AtomicDb for DummyAtomicDb {
        type State = DummyState;
        type Mutation = DummyMutation;
        type Error = ();

        fn get_state(&self) -> &Self::State {
            &self.dummy_state
        }

        fn mutate_db(&mut self, mutations: &[Self::Mutation]) -> Result<(), Self::Error> {
            for mutation in mutations {
                match mutation {
                    DummyMutation::Inc => {
                        self.dummy_state.x = self.dummy_state.x.saturating_add(1);
                    },
                    DummyMutation::Dec => {
                        self.dummy_state.x = self.dummy_state.x.saturating_sub(1);
                    },
                };
            }
            Ok(())
        }
    }

    async fn task_database_loop_basic<S>(mut spawner: S) 
    where
        S: Spawn,
    {
        let atomic_db = DummyAtomicDb::new();
        let (request_sender, incoming_requests) = mpsc::channel(0);
        let loop_fut = database_loop(atomic_db, incoming_requests);
        let loop_res_fut = spawner.spawn_with_handle(loop_fut).unwrap();

        let mut db_client = DatabaseClient::new(request_sender);
        await!(db_client.mutate(vec![DummyMutation::Inc, 
                              DummyMutation::Inc,
                              DummyMutation::Dec])).unwrap();

        await!(db_client.mutate(vec![DummyMutation::Inc, 
                              DummyMutation::Inc,
                              DummyMutation::Dec])).unwrap();

        // Droping the only client should close the loop:
        drop(db_client);

        let atomic_db = await!(loop_res_fut).unwrap();
        assert_eq!(atomic_db.dummy_state.x, 1 + 1 - 1 + 1 + 1 - 1);
    }

    #[test]
    fn test_database_loop_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_database_loop_basic(thread_pool.clone()));
    }
}
