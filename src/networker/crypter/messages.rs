use futures::sync::oneshot;

use super::super::messages::{RequestSendMessage, ResponseSendMessage};
use proto::indexer::NeighborsRoute;


pub struct CrypterRequestSendMessage {
    route: NeighborsRoute,
    request_send_message: RequestSendMessage,
    response_sender: oneshot::Sender<ResponseSendMessage>,
}

