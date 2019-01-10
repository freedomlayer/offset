use std::hash::Hash;
use im::hashmap::HashMap as ImHashMap;

use crypto::uid::Uid;
use common::safe_arithmetic::SafeSignedArithmetic;

use proto::funder::messages::{PendingRequest, RequestsStatus, TPublicKey};

/// The maximum possible funder debt.
/// We don't use the full u128 because i128 can not go beyond this value.
pub const MAX_FUNDER_DEBT: u128 = (1 << 127) - 1;


// TODO: Rename this to McIdents
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McIdents<P> {
    /// My public key
    pub local_public_key: TPublicKey<P>,
    /// Friend's public key
    pub remote_public_key: TPublicKey<P>,
}

// TODO: Rename this to McBalance
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: i128,
    /// Maximum possible remote debt
    pub remote_max_debt: u128,
    /// Maximum possible local debt
    pub local_max_debt: u128,
    /// Frozen credits by our side
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    pub remote_pending_debt: u128,
}

impl McBalance {
    fn new(balance: i128) -> McBalance {
        McBalance {
            balance,
            /// It is still unknown what will be a good choice of initial
            /// remote_max_debt and local_max_debt here, given that balance != 0.
            /// We currently pick the simple choice of having all max_debts equal 0 initially.
            remote_max_debt: 0,
            local_max_debt: 0,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}

// TODO: Rename pending_local_requests to a shorter name, like local.

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McPendingRequests<P: Clone> {
    /// Pending requests that were opened locally and not yet completed
    pub pending_local_requests: ImHashMap<Uid, PendingRequest<P>>,
    /// Pending requests that were opened remotely and not yet completed
    pub pending_remote_requests: ImHashMap<Uid, PendingRequest<P>>,
}

impl<P> McPendingRequests<P> 
where
    P: Clone,
{
    fn new() -> Self {
        McPendingRequests {
            pending_local_requests: ImHashMap::new(),
            pending_remote_requests: ImHashMap::new(),
        }
    }
}

#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct McRequestsStatus {
    // Local is open/closed for incoming requests:
    pub local: RequestsStatus,
    // Remote is open/closed for incoming requests:
    pub remote: RequestsStatus,
}

impl McRequestsStatus {
    fn new() -> McRequestsStatus {
        McRequestsStatus {
            local: RequestsStatus::Closed,
            remote: RequestsStatus::Closed,
        }
    }
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MutualCreditState<P: Clone> {
    pub idents: McIdents<P>,
    pub balance: McBalance,
    pub pending_requests: McPendingRequests<P>,
    pub requests_status: McRequestsStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MutualCredit<P: Clone> {
    state: MutualCreditState<P>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum McMutation<P> {
    SetLocalRequestsStatus(RequestsStatus),
    SetRemoteRequestsStatus(RequestsStatus),
    SetLocalMaxDebt(u128),
    SetRemoteMaxDebt(u128),
    SetBalance(i128),
    InsertLocalPendingRequest(PendingRequest<P>),
    RemoveLocalPendingRequest(Uid),
    InsertRemotePendingRequest(PendingRequest<P>),
    RemoveRemotePendingRequest(Uid),
    SetLocalPendingDebt(u128),
    SetRemotePendingDebt(u128),
}


impl<P> MutualCredit<P> 
where
    P: Hash + Eq + Clone,
{
    pub fn new(local_public_key: &TPublicKey<P>, 
           remote_public_key: &TPublicKey<P>, 
           balance: i128) -> Self {

        MutualCredit {
            state: MutualCreditState {
                idents: McIdents {
                    local_public_key: local_public_key.clone(),
                    remote_public_key: remote_public_key.clone(),
                },
                balance: McBalance::new(balance),
                pending_requests: McPendingRequests::new(),
                requests_status: McRequestsStatus::new(),
            }
        }
    }

    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i128 {
        self.state.balance.balance
            .checked_add_unsigned(self.state.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
        // TODO: Is this the correct formula?
        // Other options:
        // *    balance
        // *    balance + remote_pending_debt - local_pending_debt
    }

    pub fn state(&self) -> &MutualCreditState<P> {
        &self.state
    }

    pub fn mutate(&mut self, tc_mutation: &McMutation<P>) {
        match tc_mutation {
            McMutation::SetLocalRequestsStatus(requests_status) => 
                self.set_local_requests_status(requests_status.clone()),
            McMutation::SetRemoteRequestsStatus(requests_status) => 
                self.set_remote_requests_status(requests_status.clone()),
            McMutation::SetLocalMaxDebt(proposed_max_debt) => 
                self.set_local_max_debt(*proposed_max_debt),
            McMutation::SetRemoteMaxDebt(proposed_max_debt) => 
                self.set_remote_max_debt(*proposed_max_debt),
            McMutation::SetBalance(balance) => 
                self.set_balance(*balance),
            McMutation::InsertLocalPendingRequest(pending_friend_request) =>
                self.insert_local_pending_request(pending_friend_request),
            McMutation::RemoveLocalPendingRequest(request_id) =>
                self.remove_local_pending_request(request_id),
            McMutation::InsertRemotePendingRequest(pending_friend_request) =>
                self.insert_remote_pending_request(pending_friend_request),
            McMutation::RemoveRemotePendingRequest(request_id) =>
                self.remove_remote_pending_request(request_id),
            McMutation::SetLocalPendingDebt(local_pending_debt) =>
                self.set_local_pending_debt(*local_pending_debt),
            McMutation::SetRemotePendingDebt(remote_pending_debt) =>
                self.set_remote_pending_debt(*remote_pending_debt),
        }
    }

    fn set_local_requests_status(&mut self, requests_status: RequestsStatus) {
        self.state.requests_status.local = requests_status;
    }

    fn set_remote_requests_status(&mut self, requests_status: RequestsStatus) {
        self.state.requests_status.remote = requests_status;
    }

    fn set_remote_max_debt(&mut self, proposed_max_debt: u128) { 
        self.state.balance.remote_max_debt = proposed_max_debt;
    }

    fn set_local_max_debt(&mut self, proposed_max_debt: u128) {
        self.state.balance.local_max_debt = proposed_max_debt;
    }

    fn set_balance(&mut self, balance: i128) {
        self.state.balance.balance = balance;
    }

    fn insert_remote_pending_request(&mut self, pending_friend_request: &PendingRequest<P>) {
        self.state.pending_requests.pending_remote_requests.insert(
            pending_friend_request.request_id,
            pending_friend_request.clone());
    }

    fn remove_remote_pending_request(&mut self, request_id: &Uid) {
        let _ = self.state.pending_requests.pending_remote_requests.remove(
            request_id);
    }

    fn insert_local_pending_request(&mut self, pending_friend_request: &PendingRequest<P>) {
        self.state.pending_requests.pending_local_requests.insert(
            pending_friend_request.request_id,
            pending_friend_request.clone());
    }

    fn remove_local_pending_request(&mut self, request_id: &Uid) {
        let _ = self.state.pending_requests.pending_local_requests.remove(
            request_id);
    }

    fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) {
        self.state.balance.remote_pending_debt = remote_pending_debt;
    }


    fn set_local_pending_debt(&mut self, local_pending_debt: u128) {
        self.state.balance.local_pending_debt = local_pending_debt;
    }
}
