use im::hashmap::HashMap as ImHashMap;
use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;

use common::safe_arithmetic::SafeSignedArithmetic;
use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::funder::messages::{
    PendingTransaction, RequestSendFundsOp, RequestsStatus, TransactionStage,
};

/// The maximum possible funder debt.
/// We don't use the full u128 because i128 can not go beyond this value.
pub const MAX_FUNDER_DEBT: u128 = (1 << 127) - 1;

// TODO: Rename this to McIdents
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McIdents {
    /// My public key
    pub local_public_key: PublicKey,
    /// Friend's public key
    pub remote_public_key: PublicKey,
}

// TODO: Rename this to McBalance
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: i128,
    /// Maximum possible local debt
    pub local_max_debt: u128,
    /// Maximum possible remote debt
    pub remote_max_debt: u128,
    /// Frozen credits by our side
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    pub remote_pending_debt: u128,
}

impl McBalance {
    fn new(balance: i128) -> McBalance {
        McBalance {
            balance,
            local_max_debt: 0,
            /// It is still unknown what will be a good choice of initial
            /// remote_max_debt and local_max_debt here, given that balance != 0.
            /// We currently pick the simple choice of having all max_debts equal 0 initially.
            remote_max_debt: 0,
            local_pending_debt: 0,
            remote_pending_debt: 0,
        }
    }
}

// TODO: Rename pending_local_requests to a shorter name, like local.

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McPendingTransactions {
    /// Pending requests that were opened locally and not yet completed
    pub pending_local_requests: ImHashMap<Uid, PendingTransaction>,
    /// Pending requests that were opened remotely and not yet completed
    pub pending_remote_requests: ImHashMap<Uid, PendingTransaction>,
}

impl McPendingTransactions {
    fn new() -> McPendingTransactions {
        McPendingTransactions {
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

/// Rates for forwarding a transaction
/// For a transaction of `x` credits, the amount of fees will be:
/// `(x * mul) / 2^32 + add`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rate {
    /// Commission
    mul: u32,
    /// Flat rate
    add: u32,
}

#[derive(Debug)]
pub struct RateError;

impl Rate {
    pub fn new() -> Self {
        Rate { mul: 0, add: 0 }
    }

    pub fn calc_fee(&self, dest_payment: u128) -> Result<u128, RateError> {
        let mul_res = (BigUint::from(dest_payment) * BigUint::from(self.mul)) >> 32;
        let res = mul_res + BigUint::from(self.add);
        res.to_u128().ok_or(RateError)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MutualCreditState {
    /// Public identities of local and remote side
    pub idents: McIdents,
    /// Rate for forwarding new transactions to the remote side
    pub rate: Rate,
    /// Current credit balance with respect to remote side
    pub balance: McBalance,
    /// Requests in progress
    pub pending_transactions: McPendingTransactions,
    /// Can local or remote side open requests?
    /// We can allow or disallow opening new requests from the remote side to our side.
    /// The remote side controls the opposite direction.
    pub requests_status: McRequestsStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MutualCredit {
    state: MutualCreditState,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum McMutation {
    SetLocalRequestsStatus(RequestsStatus),
    SetRemoteRequestsStatus(RequestsStatus),
    SetLocalMaxDebt(u128),
    SetRemoteMaxDebt(u128),
    SetBalance(i128),
    InsertLocalPendingTransaction(PendingTransaction),
    RemoveLocalPendingTransaction(Uid),
    SetLocalPendingTransactionStage((Uid, TransactionStage)),
    InsertRemotePendingTransaction(PendingTransaction),
    RemoveRemotePendingTransaction(Uid),
    SetRemotePendingTransactionStage((Uid, TransactionStage)),
    SetLocalPendingDebt(u128),
    SetRemotePendingDebt(u128),
    SetRate(Rate),
}

impl MutualCredit {
    pub fn new(
        local_public_key: &PublicKey,
        remote_public_key: &PublicKey,
        balance: i128,
    ) -> MutualCredit {
        MutualCredit {
            state: MutualCreditState {
                idents: McIdents {
                    local_public_key: local_public_key.clone(),
                    remote_public_key: remote_public_key.clone(),
                },
                rate: Rate::new(),
                balance: McBalance::new(balance),
                pending_transactions: McPendingTransactions::new(),
                requests_status: McRequestsStatus::new(),
            },
        }
    }

    /// Calculate required balance for reset.
    /// This would be current balance plus additional future profits.
    pub fn balance_for_reset(&self) -> i128 {
        self.state
            .balance
            .balance
            .checked_add_unsigned(self.state.balance.remote_pending_debt)
            .expect("Overflow when calculating balance_for_reset")
        // TODO: Is this the correct formula?
        // Other options:
        // *    balance
        // *    balance + remote_pending_debt - local_pending_debt
    }

    pub fn state(&self) -> &MutualCreditState {
        &self.state
    }

    pub fn mutate(&mut self, tc_mutation: &McMutation) {
        match tc_mutation {
            McMutation::SetLocalRequestsStatus(requests_status) => {
                self.set_local_requests_status(requests_status.clone())
            }
            McMutation::SetRemoteRequestsStatus(requests_status) => {
                self.set_remote_requests_status(requests_status.clone())
            }
            McMutation::SetLocalMaxDebt(proposed_max_debt) => {
                self.set_local_max_debt(*proposed_max_debt)
            }
            McMutation::SetRemoteMaxDebt(proposed_max_debt) => {
                self.set_remote_max_debt(*proposed_max_debt)
            }
            McMutation::SetBalance(balance) => self.set_balance(*balance),
            McMutation::InsertLocalPendingTransaction(pending_friend_request) => {
                self.insert_local_pending_transaction(pending_friend_request)
            }
            McMutation::RemoveLocalPendingTransaction(request_id) => {
                self.remove_local_pending_transaction(request_id)
            }
            McMutation::SetLocalPendingTransactionStage((request_id, stage)) => {
                self.set_local_pending_transaction_stage(&request_id, stage.clone())
            }
            McMutation::InsertRemotePendingTransaction(pending_friend_request) => {
                self.insert_remote_pending_transaction(pending_friend_request)
            }
            McMutation::RemoveRemotePendingTransaction(request_id) => {
                self.remove_remote_pending_transaction(request_id)
            }
            McMutation::SetRemotePendingTransactionStage((request_id, stage)) => {
                self.set_remote_pending_transaction_stage(&request_id, stage.clone())
            }
            McMutation::SetLocalPendingDebt(local_pending_debt) => {
                self.set_local_pending_debt(*local_pending_debt)
            }
            McMutation::SetRemotePendingDebt(remote_pending_debt) => {
                self.set_remote_pending_debt(*remote_pending_debt)
            }
            McMutation::SetRate(rate) => self.set_rate(rate.clone()),
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

    fn insert_remote_pending_transaction(&mut self, pending_friend_request: &PendingTransaction) {
        self.state
            .pending_transactions
            .pending_remote_requests
            .insert(
                pending_friend_request.request_id,
                pending_friend_request.clone(),
            );
    }

    fn remove_remote_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self
            .state
            .pending_transactions
            .pending_remote_requests
            .remove(request_id);
    }

    fn insert_local_pending_transaction(&mut self, pending_friend_request: &PendingTransaction) {
        self.state
            .pending_transactions
            .pending_local_requests
            .insert(
                pending_friend_request.request_id,
                pending_friend_request.clone(),
            );
    }

    fn remove_local_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self
            .state
            .pending_transactions
            .pending_local_requests
            .remove(request_id);
    }

    fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) {
        self.state.balance.remote_pending_debt = remote_pending_debt;
    }

    fn set_local_pending_debt(&mut self, local_pending_debt: u128) {
        self.state.balance.local_pending_debt = local_pending_debt;
    }

    fn set_rate(&mut self, rate: Rate) {
        self.state.rate = rate;
    }
    /*
    /// Keep information from a RequestSendFunds message.
    /// This information will be used later to deal with a corresponding {Response,Failure}SendFunds messages,
    /// as those messages do not repeat the information sent in the request.
    fn create_pending_transaction(
        &self,
        request_send_funds: &RequestSendFundsOp,
    ) -> Result<PendingTransaction, RateError> {
        Ok(PendingTransaction {
            request_id: request_send_funds.request_id,
            route: request_send_funds.route.clone(),
            dest_payment: request_send_funds.dest_payment,
            invoice_id: request_send_funds.invoice_id.clone(),
            fee: self.state.rate.calc_fee(request_send_funds.dest_payment)?,
            src_hashed_lock: request_send_funds.src_hashed_lock.clone(),
            stage: TransactionStage::Request,
        })
    }
    */
    fn set_local_pending_transaction_stage(&mut self, request_id: &Uid, stage: TransactionStage) {
        self.state
            .pending_transactions
            .pending_local_requests
            .get_mut(&request_id)
            .unwrap()
            .stage = stage;
    }
    fn set_remote_pending_transaction_stage(&mut self, request_id: &Uid, stage: TransactionStage) {
        self.state
            .pending_transactions
            .pending_remote_requests
            .get_mut(&request_id)
            .unwrap()
            .stage = stage;
    }
}

pub fn create_pending_transaction(request_send_funds: &RequestSendFundsOp) -> PendingTransaction {
    PendingTransaction {
        request_id: request_send_funds.request_id,
        route: request_send_funds.route.clone(),
        dest_payment: request_send_funds.dest_payment,
        invoice_id: request_send_funds.invoice_id.clone(),
        left_fees: request_send_funds.left_fees,
        src_hashed_lock: request_send_funds.src_hashed_lock.clone(),
        stage: TransactionStage::Request,
    }
}
