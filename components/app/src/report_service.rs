use futures::{future, stream, StreamExt};
use futures::channel::{mpsc, oneshot};

use common::mutable_state::MutableState;

/// A present state and a receiver of future mutations
pub type StateResponse<ST,MU> = (ST, mpsc::Receiver<MU>);

#[derive(Debug)]
pub enum StateServiceError {
    IncomingRequestsClosed,
    IncomingMutationsClosed,
}

pub struct StateRequest<ST,MU> {
    response_sender: oneshot::Sender<StateResponse<ST,MU>>
}

pub struct StateClient<ST,MU> {
    request_sender: mpsc::Sender<StateRequest<ST,MU>>,
}

enum Event<ST,MU> {
    IncomingRequest(StateRequest<ST,MU>),
    IncomingRequestsClosed,
    IncomingMutation(MU),
    IncomingMutationsClosed,
}


pub async fn state_service<ST,MU>(incoming_requests: mpsc::Receiver<StateRequest<ST,MU>>,
                                  _state: ST,
                                  incoming_mutations: mpsc::Receiver<MU>) 
                            -> Result<(), StateServiceError>
where
    ST: MutableState<Mutation=MU>,
{
    let incoming_requests = incoming_requests
        .map(|request| Event::IncomingRequest(request))
        .chain(stream::once(future::ready(Event::IncomingRequestsClosed)));

    let incoming_mutations = incoming_mutations
        .map(|mutation| Event::IncomingMutation(mutation))
        .chain(stream::once(future::ready(Event::IncomingMutationsClosed)));

    let mut incoming = incoming_requests.select(incoming_mutations);

    while let Some(event) = await!(incoming.next()) {
        match event {
            Event::IncomingRequest(_request) => unimplemented!(),
            Event::IncomingRequestsClosed => return Err(StateServiceError::IncomingRequestsClosed),
            Event::IncomingMutation(_mutation) => unimplemented!(),
            Event::IncomingMutationsClosed => return Err(StateServiceError::IncomingMutationsClosed),
        }
    }
    Ok(())
}
