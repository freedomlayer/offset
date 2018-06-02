use futures::sync::oneshot;

use super::super::messages::{RequestSendMessage, ResponseSendMessage};
use networker::messenger::messenger_messages::NeighborsRoute;


pub struct CrypterRequestSendMessage {
    route: NeighborsRoute,
    request_send_message: RequestSendMessage,
    response_sender: oneshot::Sender<ResponseSendMessage>,
}

