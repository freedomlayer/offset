use std::cmp;

use im::hashmap::HashMap as ImHashMap;

// use num_bigint::BigUint;

use crypto::identity::PublicKey;
use crypto::uid::Uid;
use crypto::rand_values::{RandValue};

use proto::funder::InvoiceId;
use proto::networker::{NetworkerSendPrice, ChannelToken};

use utils::safe_arithmetic::SafeArithmetic;

use super::super::types::{PendingNeighborRequest, NeighborTcOp};

/// The maximum possible networker debt.
/// We don't use the full u64 because i64 can not go beyond this value.
pub const MAX_NETWORKER_DEBT: u64 = (1 << 63) - 1;

#[derive(Clone)]
pub struct NeighborMoveTokenInner {
    pub operations: Vec<NeighborTcOp>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
}

#[derive(Clone)]
pub struct TcIdents {
    /// My public key
    pub(super) local_public_key: PublicKey,
    /// Neighbor's public key
    pub(super) remote_public_key: PublicKey,
}

#[derive(Clone)]
pub struct TcBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub(super) balance: i64,
    /// Maximum possible remote debt
    pub(super) remote_max_debt: u64,
    /// Maximum possible local debt
    pub(super) local_max_debt: u64,
    /// Frozen credits by our side
    pub(super) local_pending_debt: u64,
    /// Frozen credits by the remote side
    pub(super) remote_pending_debt: u64,
}

impl TcBalance {
    fn new(balance: i64) -> TcBalance {
        TcBalance {
            balance,
            remote_max_debt: cmp::max(balance, 0) as u64,
            local_max_debt: cmp::min(-balance, 0) as u64,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}


#[derive(Clone)]
pub struct TcInvoice {
    /// The invoice id which I randomized locally
    pub(super) local_invoice_id: Option<InvoiceId>,
    /// The invoice id which the neighbor randomized
    pub(super) remote_invoice_id: Option<InvoiceId>,
}

impl TcInvoice {
    fn new() -> TcInvoice {
        TcInvoice {
            local_invoice_id: None,
            remote_invoice_id: None,
        }
    }
}

#[derive(Clone)]
pub struct TcSendPrice {
    /// Price for us to send message to the remote side
    /// Known only if we enabled requests
    pub(super) local_send_price: Option<NetworkerSendPrice>,
    /// Price for the remote side to send messages to us
    /// Known only if remote side enabled requests
    pub(super) remote_send_price: Option<NetworkerSendPrice>,
}

impl TcSendPrice {
    fn new() -> TcSendPrice {
        TcSendPrice {
            local_send_price: None,
            remote_send_price: None,
        }
    }
}

#[derive(Clone)]
pub struct TcPendingRequests {
    /// Pending requests that were opened locally and not yet completed
    pub(super) pending_local_requests: ImHashMap<Uid, PendingNeighborRequest>,
    /// Pending requests that were opened remotely and not yet completed
    pub(super) pending_remote_requests: ImHashMap<Uid, PendingNeighborRequest>,
}

impl TcPendingRequests {
    fn new() -> TcPendingRequests {
        TcPendingRequests {
            pending_local_requests: ImHashMap::new(),
            pending_remote_requests: ImHashMap::new(),
        }
    }
}


#[derive(Clone)]
pub struct TokenChannelState {
    pub idents: TcIdents,
    pub balance: TcBalance,
    pub invoice: TcInvoice,
    pub send_price: TcSendPrice,
    pub pending_requests: TcPendingRequests,
}

#[derive(Clone)]
pub struct TokenChannel {
    state: TokenChannelState,
}

pub enum TcMutation {
    SetLocalSendPrice(NetworkerSendPrice),
    ClearLocalSendPrice,
    SetRemoteSendPrice(NetworkerSendPrice),
    ClearRemoteSendPrice,
    SetLocalMaxDebt(u64),
    SetRemoteMaxDebt(u64),
    SetLocalInvoiceId(InvoiceId),
    ClearLocalInvoiceId,
    SetRemoteInvoiceId(InvoiceId),
    ClearRemoteInvoiceId,
    SetBalance(i64),
    InsertLocalPendingRequest(PendingNeighborRequest),
    RemoveLocalPendingRequest(Uid),
    InsertRemotePendingRequest(PendingNeighborRequest),
    RemoveRemotePendingRequest(Uid),
    SetLocalPendingDebt(u64),
    SetRemotePendingDebt(u64),
}

pub enum TcMutateError {
}

impl TokenChannel {
    pub fn new(local_public_key: &PublicKey, 
           remote_public_key: &PublicKey, 
           balance: i64) -> TokenChannel {

        TokenChannel {
            state: TokenChannelState {
                idents: TcIdents {
                    local_public_key: local_public_key.clone(),
                    remote_public_key: remote_public_key.clone(),
                },
                balance: TcBalance::new(balance),
                invoice: TcInvoice::new(),
                send_price: TcSendPrice::new(),
                pending_requests: TcPendingRequests::new(),
            }
        }
    }

    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i64 {
        self.state.balance.balance
            .checked_add_unsigned(self.state.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
    }

    pub fn state(&self) -> &TokenChannelState {
        &self.state
    }

    pub fn mutate(&mut self, tc_mutation: &TcMutation) {
        match tc_mutation {
            TcMutation::SetLocalSendPrice(send_price) =>
                self.set_local_send_price(send_price),
            TcMutation::ClearLocalSendPrice =>
                self.clear_local_send_price(),
            TcMutation::SetRemoteSendPrice(send_price) => 
                self.set_remote_send_price(send_price),
            TcMutation::ClearRemoteSendPrice => 
                self.clear_remote_send_price(),
            TcMutation::SetLocalMaxDebt(proposed_max_debt) => 
                self.set_local_max_debt(*proposed_max_debt),
            TcMutation::SetRemoteMaxDebt(proposed_max_debt) => 
                self.set_remote_max_debt(*proposed_max_debt),
            TcMutation::SetLocalInvoiceId(invoice_id) => 
                self.set_local_invoice_id(invoice_id),
            TcMutation::ClearLocalInvoiceId => 
                self.clear_local_invoice_id(),
            TcMutation::SetRemoteInvoiceId(invoice_id) => 
                self.set_remote_invoice_id(invoice_id),
            TcMutation::ClearRemoteInvoiceId => 
                self.clear_remote_invoice_id(),
            TcMutation::SetBalance(balance) => 
                self.set_balance(*balance),
            TcMutation::InsertLocalPendingRequest(pending_neighbor_request) =>
                self.insert_local_pending_request(pending_neighbor_request),
            TcMutation::RemoveLocalPendingRequest(request_id) =>
                self.remove_local_pending_request(request_id),
            TcMutation::InsertRemotePendingRequest(pending_neighbor_request) =>
                self.insert_remote_pending_request(pending_neighbor_request),
            TcMutation::RemoveRemotePendingRequest(request_id) =>
                self.remove_remote_pending_request(request_id),
            TcMutation::SetLocalPendingDebt(local_pending_debt) =>
                self.set_local_pending_debt(*local_pending_debt),
            TcMutation::SetRemotePendingDebt(remote_pending_debt) =>
                self.set_remote_pending_debt(*remote_pending_debt),
        }
    }

    fn set_remote_send_price(&mut self, send_price: &NetworkerSendPrice) {
        self.state.send_price.remote_send_price = Some(send_price.clone());
    }

    fn clear_remote_send_price(&mut self) {
        self.state.send_price.remote_send_price = None;
    }

    fn set_remote_max_debt(&mut self, proposed_max_debt: u64) { 
        self.state.balance.remote_max_debt = proposed_max_debt;
    }

    fn set_local_max_debt(&mut self, proposed_max_debt: u64) {
        self.state.balance.local_max_debt = proposed_max_debt;
    }

    fn set_remote_invoice_id(&mut self, invoice_id: &InvoiceId) {
        self.state.invoice.remote_invoice_id = Some(invoice_id.clone());
    }

    fn clear_remote_invoice_id(&mut self) {
        self.state.invoice.remote_invoice_id = None;
    }

    fn set_local_invoice_id(&mut self, invoice_id: &InvoiceId) {
        self.state.invoice.local_invoice_id = Some(invoice_id.clone());
    }

    fn clear_local_invoice_id(&mut self) {
        self.state.invoice.local_invoice_id = None;
    }

    fn set_balance(&mut self, balance: i64) {
        self.state.balance.balance = balance;
    }

    fn insert_remote_pending_request(&mut self, pending_neighbor_request: &PendingNeighborRequest) {
        self.state.pending_requests.pending_remote_requests.insert(
            pending_neighbor_request.request_id.clone(),
            pending_neighbor_request.clone());
    }

    fn remove_remote_pending_request(&mut self, request_id: &Uid) {
        let _ = self.state.pending_requests.pending_remote_requests.remove(
            request_id);
    }

    fn insert_local_pending_request(&mut self, pending_neighbor_request: &PendingNeighborRequest) {
        self.state.pending_requests.pending_local_requests.insert(
            pending_neighbor_request.request_id.clone(),
            pending_neighbor_request.clone());
    }

    fn remove_local_pending_request(&mut self, request_id: &Uid) {
        let _ = self.state.pending_requests.pending_local_requests.remove(
            request_id);
    }

    fn set_remote_pending_debt(&mut self, remote_pending_debt: u64) {
        self.state.balance.remote_pending_debt = remote_pending_debt;
    }


    fn set_local_pending_debt(&mut self, local_pending_debt: u64) {
        self.state.balance.local_pending_debt = local_pending_debt;
    }

    fn set_local_send_price(&mut self, send_price: &NetworkerSendPrice) {
        self.state.send_price.local_send_price = Some(send_price.clone());
    }

    fn clear_local_send_price(&mut self) {
        self.state.send_price.local_send_price = None;
    }

}

