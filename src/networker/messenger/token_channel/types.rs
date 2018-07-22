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
pub struct TokenChannel {
    pub(super) idents: TcIdents,
    pub(super) balance: TcBalance,
    pub(super) invoice: TcInvoice,
    pub(super) send_price: TcSendPrice,
    pub(super) pending_requests: TcPendingRequests,
}

impl TokenChannel {
    pub fn new(local_public_key: &PublicKey, 
           remote_public_key: &PublicKey, 
           balance: i64) -> TokenChannel {

        TokenChannel {
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

    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i64 {
        self.balance.balance
            .checked_add_unsigned(self.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
    }

    pub fn pending_local_requests(&self) -> &ImHashMap<Uid, PendingNeighborRequest> {
        &self.pending_requests.pending_local_requests
    }

    pub fn pending_remote_requests(&self) -> &ImHashMap<Uid, PendingNeighborRequest> {
        &self.pending_requests.pending_remote_requests
    }
}



