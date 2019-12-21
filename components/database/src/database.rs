use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, SinkExt, StreamExt};
use std::fmt::Debug;

use crate::atomic_db::AtomicDb;

#[derive(Debug)]
pub enum DatabaseError<ADE> {
    AtomicDbError(ADE),
    SpawnError,
}

// A request to apply mutations to the database
#[derive(Debug)]
pub struct DatabaseRequest<M> {
    pub mutations: Vec<M>,
    pub response_sender: oneshot::Sender<()>,
}

#[derive(Clone, Debug)]
pub struct DatabaseClient<M> {
    request_sender: mpsc::Sender<DatabaseRequest<M>>,
}

#[derive(Debug)]
pub enum DatabaseClientError {
    SendError,
    ResponseCanceled,
}

impl<M> DatabaseClient<M>
where
    M: Debug,
{
    pub fn new(request_sender: mpsc::Sender<DatabaseRequest<M>>) -> Self {
        DatabaseClient { request_sender }
    }

    pub async fn mutate(&mut self, mutations: Vec<M>) -> Result<(), DatabaseClientError> {
        let (response_sender, request_done) = oneshot::channel();
        let database_request = DatabaseRequest {
            mutations,
            response_sender,
        };
        // Send the request:
        self.request_sender
            .send(database_request)
            .await
            .map_err(|_| DatabaseClientError::SendError)?;

        // Wait for ack from the service:
        request_done
            .await
            .map_err(|_| DatabaseClientError::ResponseCanceled)?;

        Ok(())
    }
}

pub async fn database_loop<AD, S>(
    mut atomic_db: AD,
    mut incoming_requests: mpsc::Receiver<DatabaseRequest<AD::Mutation>>,
    database_spawner: S,
) -> Result<AD, DatabaseError<AD::Error>>
where
    AD: AtomicDb + Send + 'static,
    AD::Mutation: Debug + Send + 'static,
    AD::Error: Send + 'static,
    S: Spawn,
{
    // We use an independent spawner (`database_spawner`) to make sure our synchronous interaction
    // with the database doesn't block the rest of the main thread loop.
    // TODO: Maybe there will be a better way to do this in the future (Possibly a future version
    // of async-std/Tokio that has this feature)

    while let Some(database_request) = incoming_requests.next().await {
        let DatabaseRequest {
            mutations,
            response_sender,
        } = database_request;
        let mutate_fut = future::lazy(move |_| {
            atomic_db
                .mutate_db(&mutations[..])
                .map_err(DatabaseError::AtomicDbError)?;
            Ok(atomic_db)
        });
        let handle = database_spawner
            .spawn_with_handle(mutate_fut)
            .map_err(|_| DatabaseError::SpawnError)?;

        atomic_db = handle.await?;

        // Notify client that the database mutation request was processed:
        let _ = response_sender.send(());
    }
    // Return the current state
    Ok(atomic_db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{LocalPool, ThreadPool};
    use futures::task::{Spawn, SpawnExt};

    /// A dummy state (used for testing)
    #[derive(Debug)]
    struct DummyState {
        pub x: u32,
    }

    impl DummyState {
        pub fn new() -> Self {
            DummyState { x: 0u32 }
        }
    }

    /// A dummy mutation (used for testing)
    #[derive(Debug)]
    enum DummyMutation {
        Inc,
        Dec,
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
                    }
                    DummyMutation::Dec => {
                        self.dummy_state.x = self.dummy_state.x.saturating_sub(1);
                    }
                };
            }
            Ok(())
        }
    }

    async fn task_database_loop_basic<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        let atomic_db = DummyAtomicDb::new();
        let (request_sender, incoming_requests) = mpsc::channel(0);
        let loop_fut = database_loop(atomic_db, incoming_requests, spawner.clone());
        let loop_res_fut = spawner.spawn_with_handle(loop_fut).unwrap();

        let mut db_client = DatabaseClient::new(request_sender);
        db_client
            .mutate(vec![
                DummyMutation::Inc,
                DummyMutation::Inc,
                DummyMutation::Dec,
            ])
            .await
            .unwrap();

        db_client
            .mutate(vec![
                DummyMutation::Inc,
                DummyMutation::Inc,
                DummyMutation::Dec,
            ])
            .await
            .unwrap();

        // Dropping the only client should close the loop:
        drop(db_client);

        let atomic_db = loop_res_fut.await.unwrap();
        assert_eq!(atomic_db.dummy_state.x, 1 + 1 - 1 + 1 + 1 - 1);
    }

    #[test]
    fn test_database_loop_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        LocalPool::new().run_until(task_database_loop_basic(thread_pool.clone()));
    }
}
