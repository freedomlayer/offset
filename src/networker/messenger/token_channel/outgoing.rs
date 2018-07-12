#![allow(unused)]

use std::convert::TryFrom;
use std::collections::VecDeque;

use crypto::identity::verify_signature;
use crypto::hash;

use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use proto::networker::NetworkerSendPrice;

use utils::safe_arithmetic::SafeArithmetic;
use utils::int_convert::usize_to_u32;

use super::types::{TokenChannel, TcBalance, TcInvoice, TcSendPrice, TcIdents,
    TcPendingRequests, NeighborMoveTokenInner, MAX_NETWORKER_DEBT};
use super::super::credit_calc::CreditCalculator;
use super::super::types::{NeighborTcOp, RequestSendMessage, 
    ResponseSendMessage, FailureSendMessage, PkPairPosition,
    PendingNeighborRequest, NeighborsRoute};
use super::super::signature_buff::{create_response_signature_buffer, 
    verify_failure_signature};


/// Processes outgoing messages for a token channel.
/// Used to batch as many messages as possible.
pub struct OutgoingTokenChannel {
    idents: TcIdents,
    balance: TcBalance,
    invoice: TcInvoice,
    send_price: TcSendPrice,
    pending_requests: TcPendingRequests,
    /// Accumulated operations waiting to be sent:
    operations: VecDeque<NeighborTcOp>,
}

pub enum QueueOperationError {
    RemoteMaxDebtTooLarge,
    InvoiceIdAlreadyExists,
    InvalidSendFundsReceiptSignature,
    MissingRemoteInvoiceId,
    InvoiceIdMismatch,
    DuplicateNodesInRoute,
    PkPairNotInRoute,
    InvalidFreezeLinks,
    RemoteIncomingRequestsDisabled,
    ResponsePaymentProposalTooLow,
    RouteTooLong,
    RequestContentTooLong,
    CreditCalculatorFailure,
    CreditsCalcOverflow,
    InsufficientTrust,
    RequestAlreadyExists,
    RequestDoesNotExist,
    InvalidResponseSignature,
    ProcessingFeeCollectedTooHigh,
    ResponseContentTooLong,
    ReportingNodeNonexistent,
    InvalidReportingNode,
    InvalidFailureSignature,
    FailureOriginatedFromDest,
}

pub struct QueueOperationFailure {
    operation: NeighborTcOp,
    error: QueueOperationError,
}

/// A wrapper over a token channel, accumulating messages to be sent as one transcation.
impl OutgoingTokenChannel {
    pub fn new(token_channel: TokenChannel) -> OutgoingTokenChannel {
        OutgoingTokenChannel {
            idents: token_channel.idents,
            balance: token_channel.balance,
            invoice: token_channel.invoice,
            send_price: token_channel.send_price,
            pending_requests: token_channel.pending_requests,
            operations: VecDeque::new(),
        }
    }

    /// Commit to send all the pending operations
    pub fn commit(self) -> (TokenChannel, Vec<NeighborTcOp>) {
        let token_channel = TokenChannel {
            idents: self.idents,
            balance: self.balance,
            invoice: self.invoice,
            send_price: self.send_price,
            pending_requests: self.pending_requests,
        };

        (token_channel, self.operations.into_iter().collect())
    }

    pub fn queue_operation(&mut self, operation: NeighborTcOp) ->
        Result<(), QueueOperationFailure> {
        let res = match operation.clone() {
            NeighborTcOp::EnableRequests(send_price) =>
                self.queue_enable_requests(send_price),
            NeighborTcOp::DisableRequests =>
                self.queue_disable_requests(),
            NeighborTcOp::SetRemoteMaxDebt(proposed_max_debt) =>
                self.queue_set_remote_max_debt(proposed_max_debt),
            NeighborTcOp::SetInvoiceId(rand_nonce) =>
                self.queue_set_invoice_id(rand_nonce),
            NeighborTcOp::LoadFunds(send_funds_receipt) =>
                self.queue_load_funds(send_funds_receipt),
            NeighborTcOp::RequestSendMessage(request_send_msg) =>
                self.queue_request_send_message(request_send_msg),
            NeighborTcOp::ResponseSendMessage(response_send_msg) =>
                self.queue_response_send_message(response_send_msg),
            NeighborTcOp::FailureSendMessage(failure_send_msg) =>
                self.queue_failure_send_message(failure_send_msg),
        };
        match res {
            Ok(()) => {
                self.operations.push_back(operation);
                Ok(())
            },
            Err(error) => Err(QueueOperationFailure {
                operation,
                error,
            }),
        }
    }

    pub fn is_operations_empty(&self) -> bool {
        self.operations.is_empty()
    }

    fn queue_enable_requests(&mut self, send_price: NetworkerSendPrice) ->
        Result<(), QueueOperationError> {

        // TODO: Should we check first if there is an existing send_price?
        // Currently this method is used both for enabling requests and updating the send_price.
        self.send_price.local_send_price = Some(send_price);
        Ok(())
    }

    fn queue_disable_requests(&mut self) ->
        Result<(), QueueOperationError> {
        self.send_price.local_send_price = None;
        Ok(())
    }

    fn queue_set_remote_max_debt(&mut self, proposed_max_debt: u64) -> 
        Result<(), QueueOperationError> {

        if proposed_max_debt > MAX_NETWORKER_DEBT {
            return Err(QueueOperationError::RemoteMaxDebtTooLarge);
        }

        self.balance.remote_max_debt = proposed_max_debt;
        Ok(())
    }

    fn queue_set_invoice_id(&mut self, invoice_id: InvoiceId) ->
        Result<(), QueueOperationError> {

        self.invoice.local_invoice_id = match self.invoice.local_invoice_id {
            Some(_) => return Err(QueueOperationError::InvoiceIdAlreadyExists),
            None => Some(invoice_id),
        };
        Ok(())
    }

    fn queue_load_funds(&mut self, send_funds_receipt: SendFundsReceipt) -> 
        Result<(), QueueOperationError> {

        // Verify signature:
        if !send_funds_receipt.verify_signature(&self.idents.remote_public_key) {
            return Err(QueueOperationError::InvalidSendFundsReceiptSignature);
        }

        // Make sure that the invoice id matches:
        match self.invoice.remote_invoice_id.take() {
            None => return Err(QueueOperationError::MissingRemoteInvoiceId),
            Some(invoice_id) => {
                if invoice_id != send_funds_receipt.invoice_id {
                    self.invoice.remote_invoice_id = Some(invoice_id);
                    return Err(QueueOperationError::InvoiceIdMismatch);
                }
            }
        }

        // Update balance according to payment:
        let payment = u64::try_from(send_funds_receipt.payment).unwrap_or(u64::max_value());
        self.balance.balance = self.balance.balance.saturating_add_unsigned(payment);
        Ok(())
    }

    /// Make sure that the remote side is open to incoming request
    /// and that the offered response proposal is high enough.
    fn verify_remote_send_price(&self, 
                               route: &NeighborsRoute, 
                               pk_pair_position: &PkPairPosition) 
        -> Result<(), QueueOperationError>  {

        let remote_send_price = match self.send_price.remote_send_price {
            None => Err(QueueOperationError::RemoteIncomingRequestsDisabled),
            Some(ref remote_send_price) => Ok(remote_send_price.clone()),
        }?;

        let response_proposal = match *pk_pair_position {
            PkPairPosition::Dest => &route.dest_response_proposal,
            PkPairPosition::NotDest(i) => &route.route_links[i].payment_proposal_pair.response,
        };
        // If linear payment proposal for returning response is too low, return error
        if response_proposal.smaller_than(&remote_send_price) {
            Err(QueueOperationError::ResponsePaymentProposalTooLow)
        } else {
            Ok(())
        }
    }

    fn queue_request_send_message(&mut self, request_send_msg: RequestSendMessage) ->
        Result<(), QueueOperationError> {

        // Make sure that the route does not contains cycles/duplicates:
        if !request_send_msg.route.is_cycle_free() {
            return Err(QueueOperationError::DuplicateNodesInRoute);
        }

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = request_send_msg.route.find_pk_pair(
            &self.idents.local_public_key,
            &self.idents.remote_public_key)
            .ok_or(QueueOperationError::PkPairNotInRoute)?;

        // Make sure that freeze_links and route_links are compatible in length:
        let freeze_links_len = request_send_msg.freeze_links.len();
        let route_links_len = request_send_msg.route.route_links.len();
        let is_compat = match pk_pair {
            PkPairPosition::Dest => freeze_links_len == route_links_len + 2,
            PkPairPosition::NotDest(i) => freeze_links_len == i
        };
        if !is_compat {
            return Err(QueueOperationError::InvalidFreezeLinks);
        }

        // Make sure that we have a large enough proposal for response free from the remote
        // neighbor:
        self.verify_remote_send_price(&request_send_msg.route, &pk_pair)?;

        // Calculate amount of credits to freeze.
        let request_content_len = usize_to_u32(request_send_msg.request_content.len())
            .ok_or(QueueOperationError::RequestContentTooLong)?;
        let credit_calc = CreditCalculator::new(&request_send_msg.route,
                                                request_content_len,
                                                request_send_msg.processing_fee_proposal,
                                                request_send_msg.max_response_len)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

        // Get index of remote neighbor on the route:
        let index = match pk_pair {
            PkPairPosition::Dest => request_send_msg.route.route_links.len().checked_add(1),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.ok_or(QueueOperationError::RouteTooLong)?;

        // Calculate amount of credits to freeze
        let own_freeze_credits = credit_calc.credits_to_freeze(index)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

        // Make sure we can freeze the credits
        let new_local_pending_debt = self.balance.local_pending_debt
            .checked_add(own_freeze_credits).ok_or(QueueOperationError::CreditsCalcOverflow)?;

        if new_local_pending_debt > self.balance.local_max_debt {
            return Err(QueueOperationError::InsufficientTrust);
        }

        let p_local_requests = &mut self.pending_requests.pending_local_requests;
        // Make sure that we don't have this request as a pending request already:
        if p_local_requests.contains_key(&request_send_msg.request_id) {
            return Err(QueueOperationError::RequestAlreadyExists);
        }

        // Add pending request message:
        let pending_neighbor_request = PendingNeighborRequest {
            request_id: request_send_msg.request_id,
            route: request_send_msg.route.clone(),
            request_content_hash: hash::sha_512_256(&request_send_msg.request_content),
            request_content_len,
            max_response_len: request_send_msg.max_response_len,
            processing_fee_proposal: request_send_msg.processing_fee_proposal,
        };
        p_local_requests.insert(request_send_msg.request_id,
                                     pending_neighbor_request);
        
        // If we are here, we can freeze the credits:
        self.balance.local_pending_debt = new_local_pending_debt;
        Ok(())
    }

    fn queue_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
        Result<(), QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap, 
        // and access saved request details.
        let remote_pending_requests = &self.pending_requests.pending_remote_requests;

        // Obtain pending request:
        let pending_request = remote_pending_requests.get(&response_send_msg.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?;

        // verify signature:
        let response_signature_buffer = create_response_signature_buffer(
                                            &response_send_msg,
                                            &pending_request);

        // Verify response message signature:
        if !verify_signature(&response_signature_buffer, 
                                 &self.idents.local_public_key,
                                 &response_send_msg.signature) {
            return Err(QueueOperationError::InvalidResponseSignature);
        }

        // Verify that processing_fee_collected is within range.
        if response_send_msg.processing_fee_collected > pending_request.processing_fee_proposal {
            return Err(QueueOperationError::ProcessingFeeCollectedTooHigh);
        }

        // Make sure that response_content is not longer than max_response_len.
        let response_content_len = usize_to_u32(response_send_msg.response_content.len())
            .ok_or(QueueOperationError::ResponseContentTooLong)?;
        if response_content_len > pending_request.max_response_len {
            return Err(QueueOperationError::ResponseContentTooLong)?;
        }

        let credit_calc = CreditCalculator::new(&pending_request.route,
                                                pending_request.request_content_len,
                                                pending_request.processing_fee_proposal,
                                                pending_request.max_response_len)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

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
        self.pending_requests.pending_remote_requests.remove(
            &response_send_msg.request_id);

        let success_credits = credit_calc.credits_on_success(index, response_content_len)
            .expect("credits_on_success calculation failed!");
        let freeze_credits = credit_calc.credits_to_freeze(index)
            .expect("credits_to_freeze calculation failed!");

        // Decrease frozen credits and increase balance:
        self.balance.remote_pending_debt = 
            self.balance.remote_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        self.balance.balance = 
            self.balance.balance.checked_add_unsigned(success_credits)
            .expect("balance overflow");

        Ok(())
    }

    fn queue_failure_send_message(&mut self, failure_send_msg: FailureSendMessage) ->
        Result<(), QueueOperationError> {
        // Make sure that id exists in remote_pending hashmap, 
        // and access saved request details.
        let remote_pending_requests = &self.pending_requests
            .pending_remote_requests;

        // Obtain pending request:
        let pending_request = remote_pending_requests.get(&failure_send_msg.request_id)
            .ok_or(QueueOperationError::RequestDoesNotExist)?;

        // Find ourselves on the route. If we are not there, abort.
        let pk_pair = pending_request.route.find_pk_pair(
            &self.idents.remote_public_key,
            &self.idents.local_public_key)
            .expect("Can not find myself in request's route!");

        // Note that we can not be the destination. The destination can not be the sender of a
        // failure message.
        let index = match pk_pair {
            PkPairPosition::Dest => return Err(QueueOperationError::FailureOriginatedFromDest),
            PkPairPosition::NotDest(i) => i.checked_add(1),
        }.expect("Route too long!");

        // Make sure that reporting node public key is:
        //  - inside the route
        //  - After us on the route, or us.
        //  - Not the destination node
        
        let reporting_index = pending_request.route.pk_index(
            &failure_send_msg.reporting_public_key)
            .ok_or(QueueOperationError::ReportingNodeNonexistent)?;

        let dest_index = pending_request.route.route_links.len()
            .checked_add(1)
            .ok_or(QueueOperationError::RouteTooLong)?;

        if (reporting_index < index) || (reporting_index >= dest_index) {
            return Err(QueueOperationError::InvalidReportingNode);
        }

        verify_failure_signature(index,
                                 reporting_index,
                                 &failure_send_msg,
                                 pending_request)
            .ok_or(QueueOperationError::InvalidFailureSignature)?;

        // At this point we believe the failure message is valid.

        let credit_calc = CreditCalculator::new(&pending_request.route,
                                                pending_request.request_content_len,
                                                pending_request.processing_fee_proposal,
                                                pending_request.max_response_len)
            .ok_or(QueueOperationError::CreditCalculatorFailure)?;

        // Remove entry from remote hashmap:
        self.pending_requests.pending_remote_requests.remove(
            &failure_send_msg.request_id);


        let failure_credits = credit_calc.credits_on_failure(index, reporting_index)
            .expect("credits_on_failure calculation failed!");
        let freeze_credits = credit_calc.credits_to_freeze(index)
            .expect("credits_to_freeze calculation failed!");

        // Decrease frozen credits:
        self.balance.remote_pending_debt = 
            self.balance.remote_pending_debt.checked_sub(freeze_credits)
            .expect("Insufficient frozen credit!");

        // Add to balance:
        self.balance.balance = 
            self.balance.balance.checked_add_unsigned(failure_credits)
            .expect("balance overflow");

        Ok(())
    }
}

