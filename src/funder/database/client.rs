use futures::future::Future;
use futures::Sink;
use futures::sync::{oneshot, mpsc};
use funder::state::{FunderMutation};

use super::service::IncomingMutationsBatch;

pub enum DbClientError {
    SendFailure,
    AckFailure,
}

#[derive(Clone)]
pub struct DbClient<A> {
    sender: mpsc::Sender<IncomingMutationsBatch<A>>,
}

#[allow(unused)]
impl<A> DbClient<A> {
    pub fn new(sender: mpsc::Sender<IncomingMutationsBatch<A>>) -> DbClient<A> {
        DbClient { sender }
    }

    pub fn mutate(&self, funder_mutations: Vec<FunderMutation<A>>) -> impl Future<Item=(), Error=DbClientError> {
        let (ack_sender, ack_receiver) = oneshot::channel::<()>();
        let incoming_mutations_batch = IncomingMutationsBatch {
            funder_mutations,
            ack_sender,
        };
        let sender = self.sender.clone();
        sender.send(incoming_mutations_batch)
            .map_err(|_| DbClientError::SendFailure)
            .and_then(|_| 
                  ack_receiver
                    .and_then(|_| Ok(()) )
                    .map_err(|_| DbClientError::AckFailure))
    }

}

