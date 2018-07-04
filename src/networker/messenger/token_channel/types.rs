use std::cmp;
use std::collections::HashMap;

// use num_bigint::BigUint;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::funder::InvoiceId;
use proto::networker::NetworkerSendPrice;

use utils::trans_hashmap::TransHashMap;
use utils::safe_arithmetic::SafeArithmetic;

use super::super::types::{PendingNeighborRequest};


#[derive(Clone)]
pub struct TCIdents {
    /// My public key
    pub(super) local_public_key: PublicKey,
    /// Neighbor's public key
    pub(super) remote_public_key: PublicKey,
}

#[derive(Clone)]
pub struct TCBalance {
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

impl TCBalance {
    fn new(balance: i64) -> TCBalance {
        TCBalance {
            balance,
            remote_max_debt: cmp::max(balance, 0) as u64,
            local_max_debt: cmp::min(-balance, 0) as u64,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}


#[derive(Clone)]
pub struct TCInvoice {
    /// The invoice id which I randomized locally
    pub(super) local_invoice_id: Option<InvoiceId>,
    /// The invoice id which the neighbor randomized
    pub(super) remote_invoice_id: Option<InvoiceId>,
}

impl TCInvoice {
    fn new() -> TCInvoice {
        TCInvoice {
            local_invoice_id: None,
            remote_invoice_id: None,
        }
    }
}

#[derive(Clone)]
pub struct TCSendPrice {
    /// Price for us to send message to the remote side
    /// Knowns only if we enabled requests
    pub(super) local_send_price: Option<NetworkerSendPrice>,
    /// Price for the remote side to send messages to us
    /// Knowns only if remote side enabled requests
    pub(super) remote_send_price: Option<NetworkerSendPrice>,
}

impl TCSendPrice {
    fn new() -> TCSendPrice {
        TCSendPrice {
            local_send_price: None,
            remote_send_price: None,
        }
    }
}

pub struct TCPendingRequests {
    /// Pending requests that were opened locally and not yet completed
    pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    /// Pending requests that were opened remotely and not yet completed
    pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

impl TCPendingRequests {
    fn new() -> TCPendingRequests {
        TCPendingRequests {
            pending_local_requests: HashMap::new(),
            pending_remote_requests: HashMap::new(),
        }
    }
}

pub struct TransTCPendingRequests {
    pub(super) trans_pending_local_requests: TransHashMap<Uid, PendingNeighborRequest>,
    pub(super) trans_pending_remote_requests: TransHashMap<Uid, PendingNeighborRequest>,
}

impl TransTCPendingRequests {
    pub fn new(pending_requests: TCPendingRequests) -> TransTCPendingRequests {
        TransTCPendingRequests {
            trans_pending_local_requests: TransHashMap::new(pending_requests.pending_local_requests),
            trans_pending_remote_requests: TransHashMap::new(pending_requests.pending_remote_requests),
        }
    }

    pub fn commit(self) -> TCPendingRequests {
        TCPendingRequests {
            pending_local_requests: self.trans_pending_local_requests.commit(),
            pending_remote_requests: self.trans_pending_remote_requests.commit(),
        }
    }

    pub fn cancel(self) -> TCPendingRequests {
        TCPendingRequests {
            pending_local_requests: self.trans_pending_local_requests.cancel(),
            pending_remote_requests: self.trans_pending_remote_requests.cancel(),
        }
    }
}


pub struct TokenChannel {
    pub(super) idents: TCIdents,
    pub(super) balance: TCBalance,
    pub(super) invoice: TCInvoice,
    pub(super) send_price: TCSendPrice,
    pub(super) pending_requests: TCPendingRequests,
}

impl TokenChannel {
    pub fn new(local_public_key: &PublicKey, 
           remote_public_key: &PublicKey, 
           balance: i64) -> TokenChannel {

        TokenChannel {
            idents: TCIdents {
                local_public_key: local_public_key.clone(),
                remote_public_key: remote_public_key.clone(),
            },
            balance: TCBalance::new(balance),
            invoice: TCInvoice::new(),
            send_price: TCSendPrice::new(),
            pending_requests: TCPendingRequests::new(),
        }
    }

    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i64 {
        self.balance.balance
            .checked_add_unsigned(self.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
    }

    pub fn pending_local_requests(&self) -> &HashMap<Uid, PendingNeighborRequest> {
        &self.pending_requests.pending_local_requests
    }

    pub fn pending_remote_requests(&self) -> &HashMap<Uid, PendingNeighborRequest> {
        &self.pending_requests.pending_remote_requests
    }
}



