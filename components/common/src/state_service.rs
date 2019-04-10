use futures::channel::{mpsc, oneshot};
use futures::{future, stream, SinkExt, StreamExt};

use crate::mutable_state::MutableState;

/// A present state and a receiver of future mutations
pub type StateResponse<ST, MU> = (ST, mpsc::Receiver<MU>);

#[derive(Debug)]
pub enum StateServiceError<E> {
    MutateError(E),
}

pub struct StateRequest<ST, MU> {
    response_sender: oneshot::Sender<StateResponse<ST, MU>>,
}

#[derive(Debug)]
pub enum StateClientError {
    SendRequestError,
    ReceiveResponseError,
}

#[derive(Clone)]
pub struct StateClient<ST, MU> {
    request_sender: mpsc::Sender<StateRequest<ST, MU>>,
}

impl<ST, MU> StateClient<ST, MU> {
    pub fn new(request_sender: mpsc::Sender<StateRequest<ST, MU>>) -> Self {
        StateClient { request_sender }
    }

    pub async fn request_state(&mut self) -> Result<StateResponse<ST, MU>, StateClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        let state_request = StateRequest { response_sender };

        await!(self.request_sender.send(state_request))
            .map_err(|_| StateClientError::SendRequestError)?;

        Ok(await!(response_receiver).map_err(|_| StateClientError::ReceiveResponseError)?)
    }
}

#[allow(clippy::enum_variant_names)]
enum Event<ST, MU> {
    IncomingRequest(StateRequest<ST, MU>),
    IncomingRequestsClosed,
    IncomingMutation(MU),
    IncomingMutationsClosed,
}

/// Maintain a state according to initial state and incoming mutations.
/// Serve the current state and incoming mutations to clients
pub async fn state_service<ST, MU, E>(
    incoming_requests: mpsc::Receiver<StateRequest<ST, MU>>,
    mut state: ST,
    incoming_mutations: mpsc::Receiver<MU>,
) -> Result<(), StateServiceError<E>>
where
    MU: Clone,
    ST: MutableState<Mutation = MU, MutateError = E> + Clone,
{
    let incoming_requests = incoming_requests
        .map(Event::IncomingRequest)
        .chain(stream::once(future::ready(Event::IncomingRequestsClosed)));

    let incoming_mutations = incoming_mutations
        .map(Event::IncomingMutation)
        .chain(stream::once(future::ready(Event::IncomingMutationsClosed)));

    let mut incoming = incoming_requests.select(incoming_mutations);

    let mut senders: Vec<mpsc::Sender<MU>> = Vec::new();
    let mut incoming_requests_closed: bool = false;

    while let Some(event) = await!(incoming.next()) {
        match event {
            Event::IncomingRequest(request) => {
                let (sender, receiver) = mpsc::channel(0);
                if request
                    .response_sender
                    .send((state.clone(), receiver))
                    .is_ok()
                {
                    senders.push(sender);
                }
            }
            Event::IncomingRequestsClosed => incoming_requests_closed = true,
            Event::IncomingMutation(mutation) => {
                // Mutate state:
                state
                    .mutate(&mutation)
                    .map_err(StateServiceError::MutateError)?;

                // Update all clients about state change:
                let mut new_senders = Vec::new();
                for mut sender in senders {
                    if await!(sender.send(mutation.clone())).is_ok() {
                        // We only retain the sender if no error have occurred:
                        new_senders.push(sender)
                    }
                }
                senders = new_senders;
                if senders.is_empty() && incoming_requests_closed {
                    // If all clients closed and no more requests can be received,
                    // we close the service:
                    return Ok(());
                }
            }
            Event::IncomingMutationsClosed => break,
        }
    }
    Ok(())
}

// TODO: Add tests
