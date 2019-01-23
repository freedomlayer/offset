use futures::channel::{oneshot, mpsc};
use futures::{StreamExt, SinkExt};

use crate::atomic_db::AtomicDb;

pub enum DatabaseError<ADE> {
    AtomicDbError(ADE),
}

// A request to apply mutations to the database
pub struct DatabaseRequest<M> {
    mutations: Vec<M>,
    response_sender: oneshot::Sender<()>,
}

#[derive(Clone)]
pub struct DatabaseClient<M> {
    request_sender: mpsc::Sender<DatabaseRequest<M>>,
}

pub enum DatabaseClientError {
    SendError,
    ResponseCanceled,
}

#[allow(unused)]
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

#[allow(unused)]
pub async fn database_loop<AD>(mut atomic_db: AD, 
                               mut incoming_requests: mpsc::Receiver<DatabaseRequest<AD::Mutation>>)
                                -> Result<(), DatabaseError<AD::Error>>
where
    AD: AtomicDb,
{
    while let Some(database_request) = await!(incoming_requests.next()) {
        atomic_db.mutate(&database_request.mutations[..])
            .map_err(|atomic_db_error| DatabaseError::AtomicDbError(atomic_db_error))?;

        // Notify client that the database mutation request was processed:
        let _ = database_request.response_sender.send(());
    }
    Ok(())
}

