#![allow(unused)]

use std::collections::VecDeque;

use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use proto::networker::NetworkerSendPrice;

use super::types::{TokenChannel, TcBalance, TcInvoice, TcSendPrice, TcIdents,
    TcPendingRequests, NeighborMoveTokenInner};
use super::super::types::{NeighborTcOp, RequestSendMessage, 
    ResponseSendMessage, FailureSendMessage};


/// Processes outgoing messages for a token channel.
/// Used to batch as many messages as possible.
struct OutgoingTokenChannel {
    idents: TcIdents,
    balance: TcBalance,
    invoice: TcInvoice,
    send_price: TcSendPrice,
    pending_requests: TcPendingRequests,
    /// Accumulated operations waiting to be sent:
    operations: VecDeque<NeighborTcOp>,
}

enum QueueOperationError {
}

struct QueueOperationFailure {
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

    pub fn commit(self) -> (TokenChannel, Vec<NeighborTcOp>) {
        // TODO
        unreachable!();
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
        if res.is_ok() {
            self.operations.push_back(operation);
        }
        res
    }

    fn queue_enable_requests(&mut self, send_price: NetworkerSendPrice) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_disable_requests(&mut self) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_set_remote_max_debt(&mut self, proposed_max_debt: u64) -> 
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_set_invoice_id(&mut self, invoice_id: InvoiceId) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_load_funds(&mut self, send_funds_receipt: SendFundsReceipt) -> 
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_request_send_message(&mut self, request_send_msg: RequestSendMessage) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_response_send_message(&mut self, response_send_msg: ResponseSendMessage) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

    fn queue_failure_send_message(&mut self, failure_send_msg: FailureSendMessage) ->
        Result<(), QueueOperationFailure> {
        // TODO
        unreachable!();
        Ok(())
    }

}

