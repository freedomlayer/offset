//!
//! ## Introduction
//!
//! The balance of a token channel in the networker.
//! Responsible to keep track of the debt, validating the pending debts.
//!

use std::cmp;

/// Maximum value possible for max_debt.
const MAX_NEIGHBOR_DEBT: u64 = (1 << 63) - 1;
const MIN_BALANCE: i64 = i64::min_value();
const MAX_BALANCE: i64 = i64::max_value();

/// Track the credits, allow freezing credits before actually sending them.
/// Guarantees:
///     * Avoid all integer overflows
///     * local_max_debt <= MAX_NEIGHBOR_DEBT, remote_max_debt <= MAX_NEIGHBOR_DEBT
///     * Before freezing (increasing a pending debt) potential debt credits,
///       validate:
///         balance + new_remote_pending_debt <= remote_max_debt
///         balance - new_local_pending_debt >= local_max_debt
///       Notice that these inequalities may be broken !
#[derive(Clone)]
pub struct TokenChannelCredit {
    /// maximum amount of credits I let the neighbor owe me
    pub remote_max_debt: u64,
    /// maximum amount of credits the neighbor lets me owe him
    pub local_max_debt: u64,
    /// how many pending credits the neighbor might owe me
    pub remote_pending_debt: u64,
    /// how many pending credits I might owe to the neighbor
    pub local_pending_debt: u64,
    /// Current balance - positive balance means the neighbor owes me.
    pub balance: i64,
}


impl TokenChannelCredit {
    pub fn set_remote_max_debt(&mut self, proposed_max_debt: u64) -> bool {
        if proposed_max_debt > MAX_NEIGHBOR_DEBT{
            false
        }else{
            self.remote_max_debt = proposed_max_debt;
            true
        }
    }

    pub fn set_local_max_debt(&mut self, proposed_max_debt: u64) -> bool {
        if proposed_max_debt > MAX_NEIGHBOR_DEBT{
            false
        }else{
            self.local_max_debt = proposed_max_debt;
            true
        }
    }

    // Normally called if received credits through the Funder layer.
    pub fn decrease_balance(&mut self, credits: u64) {
        self.balance = cmp::max(self.balance as i128 - credits as i128,
                                MIN_BALANCE as i128) as i64;
    }

    // Normally called if gave credits through the Funder layer.
    pub fn increase_balance(&mut self, credits: u64){
        self.balance = cmp::min(self.balance as i128 + credits as i128,
                                MAX_BALANCE as i128) as i64;
    }

    /// Freeze credits, that may be unfreezed later
    pub fn increase_remote_pending(&mut self, pending_credit_requested: u64) -> bool {
        let new_remote_pending = self.remote_pending_debt as u128 + (pending_credit_requested as u128);
        if new_remote_pending as i128 + self.balance as i128 <= self.remote_max_debt as i128{
            self.remote_pending_debt = new_remote_pending as u64;
            true
        } else {
            false
        }
    }

    /// Unfreeze total_pending_credits_to_dismiss credits, take pending_credits_to_receive credits
    /// out of them.
    /// If the new balance overflows, just take the minimum/maximum possible value.
    pub fn receive_some_from_pending_credits(&mut self, pending_credits_to_receive: u64,
                                             total_pending_credits_to_dismiss: u64) -> bool {
        if total_pending_credits_to_dismiss < pending_credits_to_receive ||
            self.remote_pending_debt < total_pending_credits_to_dismiss {
            false
        } else {
            self.remote_pending_debt -= total_pending_credits_to_dismiss;
            self.increase_balance(pending_credits_to_receive);
            true
        }
    }

    // TODO(a4vision): Make sure that it is ok that in case of an overflow in the value for balance,
    //                  we just ignore the value.
    pub fn receive_from_pending_credits(&mut self, credits: u64) -> bool{
        self.receive_some_from_pending_credits(credits, credits)
    }

    pub fn give_some_from_pending_credits(&mut self, pending_credits_to_give: u64,
                                          total_pending_credits_to_dismiss: u64) -> bool{
        if total_pending_credits_to_dismiss < pending_credits_to_give ||
            self.local_pending_debt < total_pending_credits_to_dismiss {
            false
        }else{
            self.local_pending_debt -= total_pending_credits_to_dismiss;
            self.decrease_balance(pending_credits_to_give);
            true
        }
    }

    pub fn give_from_pending_credits(&mut self, credits: u64) -> bool{
        self.give_some_from_pending_credits(credits, credits)
    }
}


