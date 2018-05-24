use std::mem;
use std::cmp;
use std::convert::TryFrom;
use std::collections::HashMap;
use byteorder::{LittleEndian, WriteBytesExt};

use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;
use crypto::hash::HashResult;

use proto::common::SendFundsReceipt;
use proto::indexer::{NeighborsRoute, PkPairPosition};
use proto::funder::InvoiceId;
use proto::networker::NetworkerSendPrice;

use super::pending_neighbor_request::PendingNeighborRequest;
use super::messenger_messages::{ResponseSendMessage, FailedSendMessage, RequestSendMessage,
                                NetworkerTCMessage};
// use super::credit_calculator::credits_on_success_dest;
use utils::trans_hashmap::TransHashMap;

/// The maximum possible networker debt.
/// We don't use the full u64 because i64 can not go beyond this value.
const MAX_NETWORKER_DEBT: u64 = (1 << 63) - 1;

pub struct IncomingResponseSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_response: ResponseSendMessage,
}

pub struct IncomingFailedSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_failed: FailedSendMessage,
}


/// Resulting tasks to perform after processing an incoming message.
/// Note that
pub enum ProcessMessageOutput {
    Request(RequestSendMessage),
    Response(IncomingResponseSendMessage),
    Failure(IncomingFailedSendMessage),
}


#[derive(Debug)]
pub enum ProcessMessageError {
    RemoteMaxDebtTooLarge(u64),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    InvoiceIdExists,
    MissingInvoiceId,
    InvalidInvoiceId,
    InvalidFundsReceipt,
    InvalidSendFundsReceiptSignature,
    PkPairNotInRoute,
    RemoteRequestIdExists,
    RequestIdNotExists,
    InvalidFeeProposal,
    PendingCreditTooLarge,
    InvalidResponseSignature,
    /// The Route contains some public key twice.
    DuplicateNodesInRoute,
    LoadFundsOverflow,
    CreditsCalculationOverflow,
    TooLongMessage,
    TooMuchFeeCollected,
    InvalidFailedSignature,
    InvalidFailureReporter,
    InnerBug,
    RequestsAlreadyDisabled,
    ResponsePaymentProposalTooLow,
    IncomingRequestsDisabled,
    RoutePricingOverflow,
}

#[derive(Debug)]
pub struct ProcessTransListError {
    index: usize,
    process_trans_error: ProcessMessageError,
}


#[derive(Clone)]
struct TCIdents {
    /// My public key
    local_public_key: PublicKey,
    /// Neighbor's public key
    remote_public_key: PublicKey,
}

#[derive(Clone)]
struct TCBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    balance: i64,
    /// Maximum possible remote debt
    remote_max_debt: u64,
    /// Maximum possible local debt
    local_max_debt: u64,
    /// Frozen credits by our side
    pending_local_debt: u64,
    /// Frozen credits by the remote side
    pending_remote_debt: u64,
}

#[derive(Clone)]
struct TCInvoice {
    /// The invoice id which I randomized locally
    local_invoice_id: Option<InvoiceId>,
    /// The invoice id which the neighbor randomized
    remote_invoice_id: Option<InvoiceId>,
}

#[derive(Clone)]
pub struct TCSendPrice {
    /// Price for us to send message to the remote side
    /// Knowns only if we enabled requests
    pub local_send_price: Option<NetworkerSendPrice>,
    /// Price for the remote side to send messages to us
    /// Knowns only if remote side enabled requests
    pub remote_send_price: Option<NetworkerSendPrice>,
}

struct TCPendingRequests {
    /// Pending requests that were opened locally and not yet completed
    pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    /// Pending requests that were opened remotely and not yet completed
    pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

struct TransTCPendingRequests {
    trans_pending_local_requests: TransHashMap<Uid, PendingNeighborRequest>,
    trans_pending_remote_requests: TransHashMap<Uid, PendingNeighborRequest>,
}

impl TransTCPendingRequests {
    fn new(pending_requests: TCPendingRequests) -> TransTCPendingRequests {
        TransTCPendingRequests {
            trans_pending_local_requests: TransHashMap::new(pending_requests.pending_local_requests),
            trans_pending_remote_requests: TransHashMap::new(pending_requests.pending_remote_requests),
        }
    }

    fn commit(self) -> TCPendingRequests {
        TCPendingRequests {
            pending_local_requests: self.trans_pending_local_requests.commit(),
            pending_remote_requests: self.trans_pending_remote_requests.commit(),
        }
    }

    fn cancel(self) -> TCPendingRequests {
        TCPendingRequests {
            pending_local_requests: self.trans_pending_local_requests.cancel(),
            pending_remote_requests: self.trans_pending_remote_requests.cancel(),
        }
    }
}

struct KnownNeighbor {
    max_debt: u64,
}


pub struct TokenChannel {
    known_neighbors: HashMap<PublicKey, KnownNeighbor>,
    idents: TCIdents,
    balance: TCBalance,
    invoice: TCInvoice,
    send_price: TCSendPrice,
    pending_requests: TCPendingRequests,
}



/// Processes incoming messages, acts upon an underlying `TokenChannel`.
struct TransTokenChannel {
    orig_balance: TCBalance,
    orig_invoice: TCInvoice,
    orig_send_price: TCSendPrice,
    known_neighbors: HashMap<PublicKey, KnownNeighbor>,
    idents: TCIdents,
    balance: TCBalance,
    invoice: TCInvoice,
    send_price: TCSendPrice,
    trans_pending_requests: TransTCPendingRequests,
}

/// If this function returns an error, the token channel becomes incosistent.
pub fn atomic_process_messages_list(token_channel: TokenChannel, messages: Vec<NetworkerTCMessage>)
                                    -> (TokenChannel, Result<Vec<ProcessMessageOutput>, ProcessTransListError>) {

    let mut trans_token_channel = TransTokenChannel::new(token_channel);
    match trans_token_channel.process_messages_list(messages) {
        Err(e) => (trans_token_channel.cancel(), Err(e)),
        Ok(output_tasks) => (trans_token_channel.commit(), Ok(output_tasks)),
    }
}



/// Transactional state of the token channel.
impl TransTokenChannel {
    /// original_token_channel: the underlying TokenChannel.
    pub fn new(token_channel: TokenChannel) -> TransTokenChannel {
        TransTokenChannel {
            orig_balance: token_channel.balance.clone(),
            orig_invoice: token_channel.invoice.clone(),
            orig_send_price: token_channel.send_price.clone(),
            known_neighbors: token_channel.known_neighbors,
            idents: token_channel.idents,
            balance: token_channel.balance,
            invoice: token_channel.invoice,
            send_price: token_channel.send_price,
            trans_pending_requests: TransTCPendingRequests::new(token_channel.pending_requests),
        }
    }

    pub fn cancel(self) -> TokenChannel {
        TokenChannel {
            known_neighbors: self.known_neighbors,
            idents: self.idents,
            balance: self.orig_balance,
            invoice: self.orig_invoice,
            send_price: self.orig_send_price,
            pending_requests: self.trans_pending_requests.cancel(),
        }
    }

    pub fn commit(self) -> TokenChannel {
        TokenChannel {
            known_neighbors: self.known_neighbors,
            idents: self.idents,
            balance: self.balance,
            invoice: self.invoice,
            send_price: self.send_price,
            pending_requests: self.trans_pending_requests.commit(),
        }
    }

    /// Every error is translated into an inconsistency of the token channel.
    pub fn process_messages_list(&mut self, messages: Vec<NetworkerTCMessage>) ->
        Result<Vec<ProcessMessageOutput>, ProcessTransListError> {
        let mut outputs = Vec::new();

        for (index, message) in messages.into_iter().enumerate() {
            match self.process_message(message) {
                Err(e) => return Err(ProcessTransListError {
                    index,
                    process_trans_error: e
                }),
                Ok(Some(trans_output)) => outputs.push(trans_output),
                Ok(None) => {},
            }
        }
        Ok(outputs)
    }

    fn process_message(&mut self, message: NetworkerTCMessage) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        match message {
            NetworkerTCMessage::EnableRequests(send_price) =>
                self.process_enable_requests(send_price),
            NetworkerTCMessage::DisableRequests =>
                self.process_disable_requests(),
            NetworkerTCMessage::SetRemoteMaxDebt(proposed_max_debt) =>
                self.process_set_remote_max_debt(proposed_max_debt),
            NetworkerTCMessage::SetInvoiceId(rand_nonce) =>
                self.process_set_invoice_id(rand_nonce),
            NetworkerTCMessage::LoadFunds(send_funds_receipt) =>
                self.process_load_funds(send_funds_receipt),
            NetworkerTCMessage::RequestSendMessage(request_send_msg) =>
                self.process_request_send_message(request_send_msg),
            NetworkerTCMessage::ResponseSendMessage(response_send_msg) =>
                self.process_response_send_message(response_send_msg),
            NetworkerTCMessage::FailedSendMessage(failed_send_msg) =>
                self.process_failed_send_message(failed_send_msg),
        }
    }

    fn process_enable_requests(&mut self, send_price: NetworkerSendPrice) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        self.send_price.remote_send_price = Some(send_price);
        // TODO: Should the price change be reported somewhere?
        Ok(None)
    }

    fn process_disable_requests(&mut self) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        self.send_price.remote_send_price = match self.send_price.remote_send_price {
            Some(ref send_price) => None,
            None => return Err(ProcessMessageError::RequestsAlreadyDisabled),
        };
        // TODO: Should this event be reported somehow?
        Ok(None)
    }

    fn process_set_remote_max_debt(&mut self, proposed_max_debt: u64) -> 
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        if proposed_max_debt > MAX_NETWORKER_DEBT {
            Err(ProcessMessageError::RemoteMaxDebtTooLarge(proposed_max_debt))
        } else {
            self.balance.remote_max_debt = proposed_max_debt;
            Ok(None)
        }
    }

    fn process_set_invoice_id(&mut self, invoice_id: InvoiceId)
                              -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        if self.invoice.remote_invoice_id.is_none() {
            self.invoice.remote_invoice_id = Some(invoice_id);
            Ok(None)
        } else {
            Err(ProcessMessageError::InvoiceIdExists)
        }
    }

    fn process_load_funds(&mut self, send_funds_receipt: SendFundsReceipt) -> 
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        // Check if the SendFundsReceipt is signed properly by us. 
        //      Could we somehow abstract this, for easier testing?
        if !send_funds_receipt.verify_signature(&self.idents.local_public_key) {
            return Err(ProcessMessageError::InvalidSendFundsReceiptSignature);
        }

        // Check if invoice_id matches. If so, we remove the remote invoice.
        match self.invoice.remote_invoice_id.take() {
            None => return Err(ProcessMessageError::MissingInvoiceId),
            Some(invoice_id) => {
                if invoice_id != send_funds_receipt.invoice_id {
                    self.invoice.remote_invoice_id = Some(invoice_id);
                    return Err(ProcessMessageError::InvalidInvoiceId);
                }
            }
        };

        // Add payment to self.balance.balance. We have to be careful because payment u128, and
        // self.balance.balance is of type i64.
        // TODO: Rewrite this part: Possibly simplify or refactor.
        let payment64 = match u64::try_from(send_funds_receipt.payment) {
            Ok(value) => value,
            Err(_) => u64::max_value(), // This case wasted credits for the sender.
        };
        let half64 = (payment64 / 2) as i64;
        self.balance.balance = self.balance.balance.saturating_add(half64);
        self.balance.balance = self.balance.balance.saturating_add(half64);
        self.balance.balance = self.balance.balance.saturating_add((payment64 % 2) as i64);
        Ok(None)
    }

    /// Process an incoming RequestSendMessage where we are not the destination of the route.
    fn request_send_message_not_dest(&mut self, 
                                     request_send_msg: RequestSendMessage,
                                     next_public_key: PublicKey,
                                     request_payment_proposal: NetworkerSendPrice,
                                     opt_response_payment_proposal: Option<NetworkerSendPrice>)
        -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        // If linear payment proposal for returning response is too low, return error
        let local_send_price = match self.send_price.local_send_price {
            None => return Err(ProcessMessageError::IncomingRequestsDisabled),
            Some(ref local_send_price) => local_send_price.clone(),
        };

        let response_payment_proposal = match opt_response_payment_proposal {
            None => local_send_price,
            Some(response_payment_proposal) => {
                // Beware: This is only partial order.
                // ">=" and "<" are not complements.
                if response_payment_proposal >= local_send_price {
                    response_payment_proposal
                } else {
                    return Err(ProcessMessageError::ResponsePaymentProposalTooLow);
                }
            }
        };


        /*
        let credits_freeze_dest = credits_on_success_dest(&request_send_msg.route, 
                                 &self.send_price.remote_send_price,
                                 request_send_msg.processing_fee_proposal,
                                 0,
                                 request_send_msg.max_response_len)
            .ok_or(ProcessMessageError::RoutePricingOverflow);
        */

        // TODO: Calculate the amount of credits to freeze backwards.
        // Start from the amount of credits to freeze at the destination,
        // then calculate backwards until we reach how many credits we need to freeze. 
        // Check if this value is reasonable:
        // - Avoiding freezing DoS.
        // - Are enough credits available?
        // Next, keep calculating backwards and see if all previous nodes can freeze according to
        // their freezing DoS protection.


        // TODO
        // - Calculate amount of credits to freeze
        // - Verify previous freezing links.
        //      - Verify self freezing link?
        //
        // - Make sure we can freeze the credits
        // - Freeze correct amount of credits

        unreachable!();

        // - Make sure that we can freeze the credits
        //      - Should consider relative freezing allocations (Avoiding DoS).
        // - Freeze correct amount of credits
        Err(ProcessMessageError::PendingCreditTooLarge)
    }

    /// Process an incoming RequestSendMessage where we are the destination of the route
    fn request_send_message_dest(&mut self, 
                                 request_send_msg: RequestSendMessage,
                                 opt_response_payment_proposal: Option<NetworkerSendPrice>)
        -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {


        // TODO
        unreachable!();
    }



    /// Process an incoming RequestSendMessage
    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)
        -> Result<Option<ProcessMessageOutput>, ProcessMessageError> {

        // TODO:
        // - Make sure that the route does not contains cycles/duplicates:
        if !request_send_msg.route.is_cycle_free() {
            return Err(ProcessMessageError::DuplicateNodesInRoute);
        }

        // - Make sure that we are on the route somewhere.
        // - Differentiate between the cases of being the last on the route, and being somewhere in
        //      the middle.
        let opt_pk_pair = request_send_msg.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key);
        match opt_pk_pair {
            None => Err(ProcessMessageError::PkPairNotInRoute),
            Some(PkPairPosition::NotDest {next_public_key, 
                                            request_payment_proposal, 
                                            opt_response_payment_proposal}) =>
                self.request_send_message_not_dest(request_send_msg,
                                                   next_public_key,
                                                   request_payment_proposal,
                                                   opt_response_payment_proposal),
            Some(PkPairPosition::Dest(opt_response_payment_proposal)) => 
                self.request_send_message_dest(request_send_msg,
                                               opt_response_payment_proposal),
        }
    }

    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        // TODO
        unreachable!();
    }

    fn process_failed_send_message(&mut self, failed_send_msg: FailedSendMessage) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        unreachable!();
    }
}

