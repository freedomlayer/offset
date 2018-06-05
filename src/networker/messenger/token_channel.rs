use std::convert::TryFrom;
use std::collections::HashMap;
use byteorder::{BigEndian, WriteBytesExt};

use num_bigint::BigUint;

use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;
use crypto::hash::HashResult;

use proto::common::SendFundsReceipt;
use proto::funder::InvoiceId;
use proto::networker::NetworkerSendPrice;

use super::pending_neighbor_request::PendingNeighborRequest;
use super::messenger_messages::{ResponseSendMessage, FailureSendMessage, RequestSendMessage,
                                NeighborTcOp, NetworkerFreezeLink,
                                NeighborsRoute, PkPairPosition};
use super::credit_calc::{credits_to_freeze, credits_on_success, 
    credits_on_failure, PaymentProposals};
use utils::trans_hashmap::TransHashMap;
use utils::int_convert::usize_to_u32;
use utils::safe_arithmetic::{checked_add_i64_u64, saturating_add_i64_u64};


/// The maximum possible networker debt.
/// We don't use the full u64 because i64 can not go beyond this value.
const MAX_NETWORKER_DEBT: u64 = (1 << 63) - 1;

/*
pub struct IncomingResponseSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_response: ResponseSendMessage,
}

pub struct IncomingFailedSendMessage {
    pending_request: PendingNeighborRequest,
    incoming_failed: FailedSendMessage,
}
*/


/// Resulting tasks to perform after processing an incoming message.
/// Note that
pub enum ProcessMessageOutput {
    Request(RequestSendMessage),
    Response(ResponseSendMessage),
    Failure(FailureSendMessage),
}


#[derive(Debug)]
pub enum ProcessMessageError {
    RemoteMaxDebtTooLarge(u64),
    /// Trying to set the invoiceId, while already expecting another invoice id.
    InvoiceIdExists,
    MissingInvoiceId,
    InvalidInvoiceId,
    InvalidSendFundsReceiptSignature,
    PkPairNotInRoute,
    PendingCreditTooLarge,
    /// The Route contains some public key twice.
    DuplicateNodesInRoute,
    LoadFundsOverflow,
    RequestsAlreadyDisabled,
    ResponsePaymentProposalTooLow,
    IncomingRequestsDisabled,
    RoutePricingOverflow,
    RequestContentTooLong,
    RouteTooLong,
    InsufficientTrust,
    InsufficientTransitiveTrust,
    CreditsCalcOverflow,
    IncompatibleFreezeeLinks,
    InvalidFreezeLinks,
    CreditCalculatorFailure,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ProcessingFeeCollectedTooHigh,
    ResponseContentTooLong,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
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
pub struct TCBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    balance: i64,
    /// Maximum possible remote debt
    remote_max_debt: u64,
    /// Maximum possible local debt
    local_max_debt: u64,
    /// Frozen credits by our side
    local_pending_debt: u64,
    /// Frozen credits by the remote side
    remote_pending_debt: u64,
}

#[derive(Clone)]
pub struct TCInvoice {
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


pub struct TokenChannel {
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
    idents: TCIdents,
    balance: TCBalance,
    invoice: TCInvoice,
    send_price: TCSendPrice,
    trans_pending_requests: TransTCPendingRequests,
}

/// If this function returns an error, the token channel becomes incosistent.
pub fn atomic_process_messages_list(token_channel: TokenChannel, messages: Vec<NeighborTcOp>)
                                    -> (TokenChannel, Result<Vec<ProcessMessageOutput>, ProcessTransListError>) {

    let mut trans_token_channel = TransTokenChannel::new(token_channel);
    match trans_token_channel.process_messages_list(messages) {
        Err(e) => (trans_token_channel.cancel(), Err(e)),
        Ok(output_tasks) => (trans_token_channel.commit(), Ok(output_tasks)),
    }
}


/*
fn verify_freezing_links(freeze_links: &[NetworkerFreezeLink], 
                            credit_calc: &CreditCalculator) -> Result<(), ProcessMessageError> {

    // Make sure that the freeze_links vector is valid:
    // numerator <= denominator for every link.
    for freeze_link in freeze_links {
        let usable_ratio = &freeze_link.usable_ratio;
        if usable_ratio.numerator > usable_ratio.denominator {
            return Err(ProcessMessageError::InvalidFreezeLinks);
        }
    }

    // Verify previous freezing links
    #[allow(needless_range_loop)]
    for node_findex in 0 .. freeze_links.len() {
        let freeze_link = &freeze_links[node_findex];
        let mut allowed_credits: BigUint = freeze_link.shared_credits.into();
        for fi in node_findex .. freeze_links.len() {
            allowed_credits *= freeze_link.usable_ratio.numerator;
            allowed_credits /= freeze_link.usable_ratio.denominator;
        }

        let freeze_credits = credit_calc.credits_to_freeze(node_findex)
            .ok_or(ProcessMessageError::CreditCalculatorFailure)?;

        if allowed_credits < freeze_credits.into() {
            return Err(ProcessMessageError::InsufficientTransitiveTrust);
        }
    }
    Ok(())
}
*/

/// Verify all failure message signature chain
fn verify_failure_signature(index: usize,
                            reporting_index: usize,
                            failure_send_msg: &FailureSendMessage,
                            pending_request: &PendingNeighborRequest) -> Option<()> {

    let mut failure_signature_buffer = create_failure_signature_buffer(
                                        &failure_send_msg,
                                        &pending_request);
    let next_index = index.checked_add(1)?;
    for i in (next_index ..= reporting_index).rev() {
        let sig_index = i.checked_sub(next_index)?;
        let rand_nonce = &failure_send_msg.rand_nonce_signatures[sig_index].rand_nonce;
        let signature = &failure_send_msg.rand_nonce_signatures[sig_index].signature;
        failure_signature_buffer.extend_from_slice(rand_nonce);
        let public_key = pending_request.route.pk_by_index(i)?;
        if !verify_signature(&failure_signature_buffer, public_key, signature) {
            return None;
        }
    }
    Some(())
}

struct CreditCalculator {
    payment_proposals: PaymentProposals,
    route_len: u32,
    request_content_len: u32,
    processing_fee_proposal: u64,
    max_response_len: u32
}

impl CreditCalculator {
    pub fn new(route: &NeighborsRoute, 
               request_content_len: u32,
               processing_fee_proposal: u64,
               max_response_len: u32) -> Option<Self> {

        // TODO: This might be not very efficient. 
        // Possibly optimize this in the future, maybe by passing pointers instead of cloning.
        let middle_props = route.route_links
            .iter()
            .map(|ref route_link| &route_link.payment_proposal_pair)
            .cloned()
            .collect::<Vec<_>>();

        let payment_proposals = PaymentProposals {
            middle_props,
            dest_response_proposal: route.dest_response_proposal.clone(),
        };

        Some(CreditCalculator {
            payment_proposals,
            route_len: usize_to_u32(route.route_links.len().checked_add(2)?)?,
            request_content_len,
            processing_fee_proposal,
            max_response_len,
        })
    }

    fn freeze_index_to_nodes_to_dest(&self, index: usize) -> Option<u32> {
        let index32 = usize_to_u32(index)?;
        Some(self.route_len.checked_sub(index32.checked_sub(1)?)?)
    }

    /// Calculate the amount of credits to freeze, 
    /// according to a given node index on the route.
    /// Note: source node has index 0. dest node has the last index.
    pub fn credits_to_freeze(&self, index: usize) -> Option<u64> {

        Some(credits_to_freeze(&self.payment_proposals,
            self.processing_fee_proposal,
            self.request_content_len,
            self.max_response_len,
            self.freeze_index_to_nodes_to_dest(index)?)?)
    }

    pub fn credits_on_success(&self, index: usize, 
                              response_content_len: u32) -> Option<u64> {
        Some(credits_on_success(&self.payment_proposals,
                                self.processing_fee_proposal,
                                self.request_content_len,
                                response_content_len,
                                self.max_response_len,
                                self.freeze_index_to_nodes_to_dest(index)?)?)
    }
}

/// Create the buffer we sign over at the Response message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
fn create_response_signature_buffer(response_send_msg: &ResponseSendMessage,
                        pending_request: &PendingNeighborRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(b"REQUEST_SUCCESS"));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len);
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal);
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.write_u64::<BigEndian>(response_send_msg.processing_fee_collected);
    sbuffer.extend_from_slice(&hash::sha_512_256(&response_send_msg.response_content));
    sbuffer.extend_from_slice(&response_send_msg.rand_nonce);

    sbuffer
}

/// Create the buffer we sign over at the Response message.
/// Note that the signature is not just over the Response message bytes. The signed buffer also
/// contains information from the Request message.
fn create_failure_signature_buffer(failure_send_msg: &FailureSendMessage,
                        pending_request: &PendingNeighborRequest) -> Vec<u8> {

    let mut sbuffer = Vec::new();

    // TODO: Add a const for this:
    sbuffer.extend_from_slice(&hash::sha_512_256(b"REQUEST_FAILURE"));
    sbuffer.extend_from_slice(&pending_request.request_id);
    sbuffer.write_u32::<BigEndian>(pending_request.max_response_len);
    sbuffer.write_u64::<BigEndian>(pending_request.processing_fee_proposal);
    sbuffer.extend_from_slice(&pending_request.route.hash());
    sbuffer.extend_from_slice(&pending_request.request_content_hash);
    sbuffer.extend_from_slice(&failure_send_msg.reporting_public_key);

    sbuffer
}


/// Transactional state of the token channel.
impl TransTokenChannel {
    /// original_token_channel: the underlying TokenChannel.
    pub fn new(token_channel: TokenChannel) -> TransTokenChannel {
        TransTokenChannel {
            orig_balance: token_channel.balance.clone(),
            orig_invoice: token_channel.invoice.clone(),
            orig_send_price: token_channel.send_price.clone(),
            idents: token_channel.idents,
            balance: token_channel.balance,
            invoice: token_channel.invoice,
            send_price: token_channel.send_price,
            trans_pending_requests: TransTCPendingRequests::new(token_channel.pending_requests),
        }
    }

    pub fn cancel(self) -> TokenChannel {
        TokenChannel {
            idents: self.idents,
            balance: self.orig_balance,
            invoice: self.orig_invoice,
            send_price: self.orig_send_price,
            pending_requests: self.trans_pending_requests.cancel(),
        }
    }

    pub fn commit(self) -> TokenChannel {
        TokenChannel {
            idents: self.idents,
            balance: self.balance,
            invoice: self.invoice,
            send_price: self.send_price,
            pending_requests: self.trans_pending_requests.commit(),
        }
    }

    /// Every error is translated into an inconsistency of the token channel.
    pub fn process_messages_list(&mut self, messages: Vec<NeighborTcOp>) ->
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

    fn process_message(&mut self, message: NeighborTcOp) ->
        Result<Option<ProcessMessageOutput>, ProcessMessageError> {
        match message {
            NeighborTcOp::EnableRequests(send_price) =>
                self.process_enable_requests(send_price),
            NeighborTcOp::DisableRequests =>
                self.process_disable_requests(),
            NeighborTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
                self.process_set_remote_max_debt(proposed_max_debt),
            NeighborTcOp::SetInvoiceId(rand_nonce) =>
                self.process_set_invoice_id(rand_nonce),
            NeighborTcOp::LoadFunds(send_funds_receipt) =>
                self.process_load_funds(send_funds_receipt),
            NeighborTcOp::RequestSendMessage(request_send_msg) =>
                Ok(Some(ProcessMessageOutput::Request(
                    self.process_request_send_message(request_send_msg)?))),
            NeighborTcOp::ResponseSendMessage(response_send_msg) =>
                Ok(Some(ProcessMessageOutput::Response(
                    self.process_response_send_message(response_send_msg)?))),
            NeighborTcOp::FailureSendMessage(failure_send_msg) =>
                Ok(Some(ProcessMessageOutput::Failure(
                    self.process_failure_send_message(failure_send_msg)?))),
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
        let payment = u64::try_from(send_funds_receipt.payment).unwrap_or(u64::max_value());
        self.balance.balance = saturating_add_i64_u64(self.balance.balance,
                                                   payment);
        Ok(None)
    }

    /// Make sure that we are open to incoming requests, 
    /// and that the offered response proposal is high enough.
    fn verify_local_send_price(&self, 
                               route: &NeighborsRoute, 
                               pk_pair_position: &PkPairPosition) 
        -> Result<(), ProcessMessageError>  {

        let local_send_price = match self.send_price.local_send_price {
            None => Err(ProcessMessageError::IncomingRequestsDisabled),
            Some(ref local_send_price) => Ok(local_send_price.clone()),
        }?;

        let response_proposal = match *pk_pair_position {
            PkPairPosition::Dest => &route.dest_response_proposal,
            PkPairPosition::NotDest(i) => &route.route_links[i].payment_proposal_pair.response,
        };
        // If linear payment proposal for returning response is too low, return error
        if response_proposal.smaller_than(&local_send_price) {
            Err(ProcessMessageError::ResponsePaymentProposalTooLow)
        } else {
            Ok(())
        }
    }


    /// Process an incoming RequestSendMessage
    fn process_request_send_message(&mut self, request_send_msg: RequestSendMessage)
        -> Result<RequestSendMessage, ProcessMessageError> {

        // Make sure that the route does not contains cycles/duplicates:
        if !request_send_msg.route.is_cycle_free() {
            return Err(ProcessMessageError::DuplicateNodesInRoute);
        }

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = request_send_msg.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .ok_or(ProcessMessageError::PkPairNotInRoute)?;

        // Make sure that freeze_links and route_links are compatible in length:
        let freeze_links_len = request_send_msg.freeze_links.len();
        let route_links_len = request_send_msg.route.route_links.len();
        let is_compat = match pk_pair {
            PkPairPosition::Dest => freeze_links_len == route_links_len + 2,
            PkPairPosition::NotDest(i) => freeze_links_len == i
        };
        if !is_compat {
            return Err(ProcessMessageError::InvalidFreezeLinks);
        }

        self.verify_local_send_price(&request_send_msg.route, &pk_pair)?;

        let request_content_len = usize_to_u32(request_send_msg.request_content.len())
            .ok_or(ProcessMessageError::RequestContentTooLong)?;
        let credit_calc = CreditCalculator::new(&request_send_msg.route,
                                                request_content_len,
                                                request_send_msg.processing_fee_proposal,
                                                request_send_msg.max_response_len)
            .ok_or(ProcessMessageError::CreditCalculatorFailure)?;

        // verify_freezing_links(&request_send_msg.freeze_links, &credit_calc)?;

        let index = match pk_pair {
            PkPairPosition::Dest => request_send_msg.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.ok_or(ProcessMessageError::RouteTooLong)?;

        // Calculate amount of credits to freeze
        let own_freeze_credits = credit_calc.credits_to_freeze(index)
            .ok_or(ProcessMessageError::CreditCalculatorFailure)?;

        // Make sure we can freeze the credits
        let new_remote_pending_debt = self.balance.remote_pending_debt
            .checked_add(own_freeze_credits).ok_or(ProcessMessageError::CreditsCalcOverflow)?;

        if new_remote_pending_debt > self.balance.remote_max_debt {
            return Err(ProcessMessageError::InsufficientTrust);
        }

        // Note that Verifying our own freezing link will be done outside. We don't have enough
        // information here to check this. In addition, even if it turns out we can't freeze those
        // credits, we don't want to create a token channel inconsistency.         
        
        let p_remote_requests = &mut self.trans_pending_requests.trans_pending_remote_requests;
        // Make sure that we don't have this request as a pending request already:
        if p_remote_requests.get_hmap().contains_key(&request_send_msg.request_id) {
            return Err(ProcessMessageError::RequestAlreadyExists);
        }

        // Add pending request message:
        let request_content_len = usize_to_u32(request_send_msg.request_content.len())
            .ok_or(ProcessMessageError::RequestContentTooLong)?;
        let pending_neighbor_request = PendingNeighborRequest {
            request_id: request_send_msg.request_id,
            route: request_send_msg.route.clone(),
            request_content_hash: hash::sha_512_256(&request_send_msg.request_content),
            request_content_len,
            max_response_len: request_send_msg.max_response_len,
            processing_fee_proposal: request_send_msg.processing_fee_proposal,
        };
        p_remote_requests.insert(request_send_msg.request_id,
                                     pending_neighbor_request);
        
        // If we are here, we can freeze the credits:
        self.balance.remote_pending_debt = new_remote_pending_debt;
        Ok(request_send_msg)
    }

    fn process_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
        Result<ResponseSendMessage, ProcessMessageError> {

        // Make sure that id exists in local_pending hashmap, 
        // and access saved request details.
        let local_pending_requests = self.trans_pending_requests
            .trans_pending_local_requests.get_hmap();

        // Obtain pending request:
        let pending_request = local_pending_requests.get(&response_send_msg.request_id)
            .ok_or(ProcessMessageError::RequestDoesNotExist)?;

        let response_signature_buffer = create_response_signature_buffer(
                                            &response_send_msg,
                                            &pending_request);

        // Verify response message signature:
        if !verify_signature(&response_signature_buffer, 
                                 &self.idents.local_public_key,
                                 &response_send_msg.signature) {
            return Err(ProcessMessageError::InvalidResponseSignature);
        }

        // Verify that processing_fee_collected is within range.
        if response_send_msg.processing_fee_collected > pending_request.processing_fee_proposal {
            return Err(ProcessMessageError::ProcessingFeeCollectedTooHigh);
        }

        // Make sure that response_content is not longer than max_response_len.
        let response_content_len = usize_to_u32(response_send_msg.response_content.len())
            .ok_or(ProcessMessageError::ResponseContentTooLong)?;
        if response_content_len > pending_request.max_response_len {
            return Err(ProcessMessageError::ResponseContentTooLong)?;
        }

        let credit_calc = CreditCalculator::new(&pending_request.route,
                                                pending_request.request_content_len,
                                                pending_request.processing_fee_proposal,
                                                pending_request.max_response_len)
            .ok_or(ProcessMessageError::CreditCalculatorFailure)?;

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = pending_request.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .expect("Can not find myself in request's route!");

        let index = match pk_pair {
            PkPairPosition::Dest => pending_request.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.expect("Route too long!");

        // Remove entry from local_pending hashmap:
        self.trans_pending_requests.trans_pending_local_requests.remove(
            &response_send_msg.request_id);

        let next_index = index.checked_add(1).expect("Route too long!");
        let success_credits = credit_calc.credits_on_success(next_index, response_content_len)
            .expect("credits_on_success calculation failed!");
        let freeze_credits = credit_calc.credits_to_freeze(next_index)
            .expect("credits_on_success calculation failed!");

        // Decrease frozen credits and increase balance:
        self.balance.local_pending_debt = 
            self.balance.local_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        // Increase balance
        self.balance.balance = 
            checked_add_i64_u64(self.balance.balance, success_credits)
            .expect("balance overflow");

        Ok(response_send_msg)
    }

    fn process_failure_send_message(&mut self, failure_send_msg: FailureSendMessage) ->
        Result<FailureSendMessage, ProcessMessageError> {
        
        // Make sure that id exists in local_pending hashmap, 
        // and access saved request details.
        let local_pending_requests = self.trans_pending_requests
            .trans_pending_local_requests.get_hmap();

        // Obtain pending request:
        let pending_request = local_pending_requests.get(&failure_send_msg.request_id)
            .ok_or(ProcessMessageError::RequestDoesNotExist)?;

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = pending_request.route.find_pk_pair(
            &self.idents.remote_public_key, 
            &self.idents.local_public_key)
            .expect("Can not find myself in request's route!");

        let index = match pk_pair {
            PkPairPosition::Dest => pending_request.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.expect("Route too long!");


        // Make sure that reporting node public key is:
        //  - inside the route
        //  - After us on the route.
        //  - Not the destination node
        
        let reporting_index = pending_request.route.pk_index(
            &failure_send_msg.reporting_public_key)
            .ok_or(ProcessMessageError::ReportingNodeNonexistent)?;

        let dest_index = pending_request.route.route_links.len()
            .checked_add(1)
            .ok_or(ProcessMessageError::RouteTooLong)?;

        if (reporting_index <= index) || (reporting_index >= dest_index) {
            return Err(ProcessMessageError::InvalidReportingNode);
        }

        verify_failure_signature(index,
                                 reporting_index,
                                 &failure_send_msg,
                                 pending_request)
            .ok_or(ProcessMessageError::InvalidFailureSignature)?;


        // At this point we believe the failure message is valid.
        
        // - Decrease frozen credits 
        // - Increase balance
        // - Remove entry from local_pending hashmap.
        // - Return Failure message.


        unreachable!();
    }
}
