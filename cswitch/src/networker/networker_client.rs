use futures::Future;
use ::inner_messages::NeighborsRoute;
use ::crypto::identity::PublicKey;

// NetworkerSenderClient
// ---------------------

/// A response returned from NetworkerSenderClientTrait.send_request() 
pub enum NetworkerResponse {
    Success(Vec<u8>),
    Failure,
}


/// The sending part of a Networker client.
/// Allows to send messages to remote nodes.
pub trait NetworkerSenderClientTrait {
    /// Send a request message to a remote node's Networker along a route of nodes.
    /// max_response_len allocated maximum length for the response.
    /// 
    /// processing_fee_proposal is the amount of credits proposed for the destination for the
    /// processing of the message.
    /// 
    /// half_credits_per_byte_proposal are half the amount of credits payed per byte to each of the
    /// mediators of the message.
    /// 
    /// Note: It is possible that the request will never resolve. Always use some kind of a timeout
    /// when using this API
    fn send_request<F>(&self, route: NeighborsRoute,
                            request_content: Vec<u8>,
                            max_response_len: u32,
                            processing_fee_proposal: u64,
                            half_credits_per_byte_proposal: u32) -> F
        where F: Future<Item=NetworkerResponse, Error=()>;

}

// NetworkerReceiverClient
// -----------------------

/// An incoming networker request. This request is received from the Networker. It originates from
/// some remote node. 
pub struct NetworkerIncomingRequest {
    route: NeighborsRoute, // sender_public_key is the first public key on the NeighborsRoute
    // sender_public_key: PublicKey,
    request_content: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

/// An trait for a Responder: An object used to either respond or discard a received Networker
/// request.
/// TODO: How to make sure one of the methods respond or discard are called? How to have this
/// checked by the compiler? What happens if the object is dropped before one of the methods is
/// called.
pub trait NetworkerRequestResponderTrait {
    /// Respond to the supplied request. Note that the following must be satisfied:
    /// response_content.len() <= max_response_len
    fn respond<F>(self, response_content: Vec<u8>) -> F
        where F: Future<Item=(), Error=()>;
    /// Discard the incoming request. This could happen for example if the processing_fee_proposal
    /// was too small, or the max_response_len is too small to send a reply. Discard the request
    /// will propagate an error back to the sender.
    fn discard<F>(self) -> F
        where F: Future<Item=(), Error=()>;
}

/// A receiver client of the Networker receives respondable requests: These are requests that must
/// be responded: Either by providing a Vec<u8> response, or by discarding them.
pub struct NetworkerRespondableRequest<R> {
    request: NetworkerIncomingRequest,
    responder: R,
}


