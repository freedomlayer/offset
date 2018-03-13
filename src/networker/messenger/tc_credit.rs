//!
//! ## Introduction
//!
//! The balance of a token channel in the Networker.
//! Responsible to keep track of the debt, validating the pending debts.
//!

/// Track the credits balance, allow freezing credits before actually sending them.
/// We must validate
///     `debt` + `pending_debt` <= `max_debt`
/// or else processing of some valid Response message might fail. The protocol does not incorporate
/// well with such failures.

///
/// Freezing credits does not change the balance.
/// Realizing frozen credits: after freezes credits,
///         realizing them means transforming them into an actual debt.
/// Realizing frozen credits: after freezes credits,
///         realizing them means transforming them into an actual debt.
#[derive(Clone)]
pub struct TokenChannelCredit {
    /// How many credits does my neighbor owe me
    remote_debt: Debt,
    /// How many credits do I owe my neighbor
    local_debt: Debt,
}

// TODO(a4vision): Change the style of this file to Result<(), Error>
impl TokenChannelCredit {
    pub fn new(local_max_debt: u64, remote_max_debt: u64) -> Result<TokenChannelCredit, CreditsError>{
        Ok(TokenChannelCredit {local_debt: Debt::new(local_max_debt)?,
            remote_debt: Debt::new(remote_max_debt)?})
    }

    /// How much does the neighbor owe me.
    /// Returns a negative value, if I am in debt to the neighbor.
    pub fn get_balance(&self) -> i64{
        self.remote_debt.get_debt()
    }

    // Normally called when the neighbor tries to redeem credits it sent me in the Funder layer.
    // The neighbor wants to do it in order to send me more messages in the Networker layer.
    pub fn decrease_balance(&mut self, credits: u128) -> bool {
        if credits > u128::from(u64::max_value()){
            return false;
        }
        let credits_u64 = credits as u64;
        if self.remote_debt.can_decrease_debt(credits_u64) &&
            self.local_debt.can_increase_debt(credits_u64){
            self.remote_debt.decrease_debt(credits_u64);
            self.local_debt.increase_debt(credits_u64);
            true
        }else{
            false
        }
    }

    // Normally called when I try to redeem credits after I sent them in the Funder layer.
    // I want to do it in order to send more messages in the Networker layer.
    pub fn increase_balance(&mut self, credits: u128) -> bool {
        if credits > u128::from(u64::max_value()){
            return false;
        }
        let credits_u64 = credits as u64;
        if self.remote_debt.can_increase_debt(credits_u64) &&
            self.local_debt.can_decrease_debt(credits_u64){
            self.remote_debt.increase_debt(credits_u64);
            self.local_debt.decrease_debt(credits_u64);
            true
        }else{
            false
        }
    }

    /// Freeze my credits
    // Normally called when sending a Request Message
    pub fn freeze_local_credits(&mut self, credits: u64) -> bool{
        self.local_debt.freeze_credits(credits)
    }

    /// Freeze credits of my neighbor
    // Normally called when receiving a Request Message
    pub fn freeze_remote_credits(&mut self, credits: u64) -> bool{
        self.remote_debt.freeze_credits(credits)
    }

    /// Unfreeze credits of my neighbor
    // Normally called when sending a Failure message
    pub fn unfreeze_remote_credits(&mut self, credits: u64) -> bool{
        self.remote_debt.unfreeze_credits(credits)
    }

    /// Unfreeze my credits
    // Normally called when receiving a Failure message
    pub fn unfreeze_local_credits(&mut self, credits: u64) -> bool{
        self.local_debt.unfreeze_credits(credits)
    }

    /// Realize frozen credits
    // Normally called when receiving a Response/Failure message.
    pub fn realize_local_frozen_credits(&mut self, credits: u64) -> bool {
        if self.remote_debt.can_decrease_debt(credits) &&
            self.local_debt.realize_frozen_credits(credits){
            self.remote_debt.decrease_debt(credits)

        } else {
           false
        }
    }

    /// Realize frozen credits
    // Normally called when sending a Response/ message.
    pub fn realize_remote_frozen_credits(&mut self, credits: u64) -> bool {
        if self.local_debt.can_decrease_debt(credits) &&
            self.remote_debt.realize_frozen_credits(credits){
            self.local_debt.decrease_debt(credits)
        } else {
            false
        }
    }

    pub fn set_remote_max_debt(&mut self, remote_max_debt: u64) -> bool{
        self.remote_debt.set_max_debt(remote_max_debt)
    }

    pub fn set_local_max_debt(&mut self, local_max_debt: u64) -> bool{
        self.local_debt.set_max_debt(local_max_debt)
    }

    pub fn get_remote_max_debt(&self) -> u64{
        self.remote_debt.get_max_debt()
    }

    pub fn get_local_max_debt(&self) -> u64{
        self.local_debt.get_max_debt()
    }
}

/// Debt may be < 0.
/// Guarantees:
///     * No integer overflow
///     * Pending debt is never too large
///         `pending_debt` < `i64::max_value()`
///     * Unfreezing debt is always possible, i.e.:
///         `debt` + `pending_debt` <= `i64::max_value()`
///     * (Only) Before freezing debt,
///         `debt` + `pending_debt` <= `max_debt`
#[derive(Clone)]
struct Debt{
    debt: i64,
    pending_debt: u64,
    max_debt: u64,
}


impl Debt{
    pub fn new(max_debt: u64) -> Result<Debt, CreditsError> {
        if max_debt > i64::max_value() as u64 {
            Err(CreditsError::TooLargeMaxDebt)
        } else {
            Ok(Debt {
                debt: 0,
                pending_debt: 0,
                max_debt
            })
        }
    }

    /// Check whether
    ///     new_debt + pending_debt <= i64::max_value()
    fn can_increase_debt(&self, credits: u64) -> bool {
        if credits <= i64::max_value() as u64 {
            match self.debt.checked_add(credits as i64) {
                Some(new_balance) => {
                    // Here we assume pending_debt <= i64::max_value()
                    new_balance.checked_add(self.pending_debt as i64).is_some()
                },
                None => false,
            }
        } else {
            false
        }
    }

    fn increase_debt(&mut self, credits: u64) -> bool{
        if self.can_increase_debt(credits){
            self.debt += credits as i64;
            true
        }else{
            false
        }
    }

    fn can_decrease_debt(&self, credits: u64) -> bool{
        if credits <= i64::max_value() as u64 {
            self.debt.checked_sub(credits as i64).is_some()
        }else{
            false
        }
    }

    fn decrease_debt(&mut self, credits: u64) -> bool{
        if self.can_decrease_debt(credits){
            self.debt -= credits as i64;
            true
        }else{
            false
        }
    }

    /// Make sure that
    ///     debt + new_pending_debt <= i64::max_value()
    ///     new_pending_debt <= i64::max_value()
    // TODO(a4vision): Maybe talk again after changing to use Result<>
    fn freeze_credits(&mut self, credits: u64) -> bool{
        if let Some(new_pending_debt) = self.pending_debt.checked_add(credits){
            if new_pending_debt <= i64::max_value() as u64 {
                if let Some(potential_debt) = self.debt.checked_add(new_pending_debt as i64) {
                    if potential_debt < 0 || potential_debt <= self.max_debt as i64 {
                        self.pending_debt = credits;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Dismiss some of the pending debt.
    fn unfreeze_credits(&mut self, credits: u64) -> bool{
        if self.pending_debt >= credits {
            self.pending_debt -= credits;
            true
        }else {
            false
        }
    }

    // The sum (debt + pending_debt) is not changed, therefore we don't need to validate
    //      debt + pending_debt <= i64::max_value()
    fn realize_frozen_credits(&mut self, credits: u64) -> bool{
        if self.unfreeze_credits(credits){
            self.debt += credits as i64;
            true
        }else{
            false
        }
    }

    fn set_max_debt(&mut self, max_debt: u64) -> bool{
        if max_debt <= i64::max_value() as u64{
            self.max_debt = max_debt;
            true
        }else{
            false
        }
    }

    fn get_max_debt(&self) -> u64{
        self.max_debt
    }

    fn get_debt(&self) -> i64{
        self.debt
    }

}

#[derive(Debug)]
pub enum CreditsError{
    TooLargeMaxDebt,
}


#[cfg(test)]
mod test_debt {
    use super::*;

    #[test]
    fn test_max_debt() {
        let mut debt = Debt::new(12).unwrap();
        assert_eq!(true, debt.freeze_credits(12));
        assert_eq!(false, debt.freeze_credits(1));
        assert_eq!(true, debt.set_max_debt(0));
        assert_eq!(false, debt.set_max_debt((1 << 63)));
    }

    #[test]
    fn test_redeem() {
        let mut debt = Debt::new(12).unwrap();
        assert_eq!(0, debt.get_debt());
        assert_eq!(true, debt.freeze_credits(5));
        assert_eq!(0, debt.get_debt());
        assert_eq!(true, debt.realize_frozen_credits(5));
        assert_eq!(5, debt.get_debt());
    }

    #[test]
    fn test_unfreeze() {
        let mut debt = Debt::new(12).unwrap();
        assert_eq!(true, debt.freeze_credits(12));
        assert_eq!(false, debt.freeze_credits(12));
        assert_eq!(true, debt.unfreeze_credits(12));
        assert_eq!(false, debt.unfreeze_credits(12));
        assert_eq!(true, debt.freeze_credits(12));
    }

    #[test]
    fn test_increase_debt() {
        let mut debt = Debt::new(1).unwrap();
        assert_eq!(true, debt.increase_debt(20));
        assert_eq!(20, debt.get_debt());
        assert_eq!(false, debt.increase_debt((1u64<<63)));
        assert_eq!(20, debt.get_debt());
        assert_eq!(true, debt.increase_debt((1u64<<63) - 21));
        assert_eq!(((1u64<<63) - 1) as i64, debt.get_debt());
        assert_eq!(false, debt.freeze_credits(1));
    }

    #[test]
    fn test_decrease_debt(){
        let mut debt = Debt::new(10).unwrap();
        assert_eq!(true, debt.decrease_debt(i64::max_value() as u64));
        assert_eq!(-i64::max_value(), debt.get_debt());
        assert_eq!(true, debt.increase_debt(i64::max_value() as u64));
        assert_eq!(0, debt.get_debt());
    }

    #[test]
    fn test_freezing_credits(){
        let mut debt = Debt::new(20).unwrap();
        assert_eq!(false, debt.freeze_credits(21));
        assert_eq!(true, debt.freeze_credits(20));
        assert_eq!(false, debt.unfreeze_credits(21));
        assert_eq!(true, debt.unfreeze_credits(20));
        assert_eq!(true, debt.freeze_credits(20));
    }

    #[test]
    fn test_inequality_while_freezing_credit(){
        let mut debt = Debt::new(20).unwrap();
        assert_eq!(true, debt.increase_debt(10));
        assert_eq!(false, debt.freeze_credits(11));
        assert_eq!(true, debt.freeze_credits(10));
    }

    #[test]
    fn test_overflow_while_increasing_debt(){
        let mut debt = Debt::new(10).unwrap();
        assert_eq!(true, debt.freeze_credits(10));
        assert_eq!(false, debt.increase_debt(i64::max_value() as u64));
    }
}

#[cfg(test)]
mod test_channel_credit {
    use super::*;
    use rand::Rng;
    use rand::distributions::Range;
    extern crate rand;
    use rand::distributions::Sample;

    fn is_consistent(credit:&TokenChannelCredit) -> bool{
        credit.remote_debt.get_debt() == -credit.local_debt.get_debt()
    }

    #[test]
    fn test_basic(){
        let mut credit = TokenChannelCredit::new(10, 10).unwrap();
        assert_eq!(true, credit.increase_balance(100));
        assert_eq!(100, credit.get_balance());
        assert_eq!(false, credit.freeze_remote_credits(1));
        assert_eq!(true, credit.freeze_local_credits(100 + 10));
        assert_eq!(100, credit.get_balance());
        assert_eq!(true, credit.realize_local_frozen_credits(100 + 10));
        assert_eq!(-10, credit.get_balance());
    }

    #[test]
    fn test_consistency_while_changing_balance(){
        let mut credit = TokenChannelCredit::new(10, 10).unwrap();
        assert_eq!(true, is_consistent(&credit));
        assert_eq!(true, credit.increase_balance(100));
        assert_eq!(true, is_consistent(&credit));
        assert_eq!(100, credit.get_balance());
        assert_eq!(true, is_consistent(&credit));
        assert_eq!(false, credit.freeze_remote_credits(1));
        assert_eq!(true, is_consistent(&credit));
        assert_eq!(true, credit.freeze_local_credits(100 + 10));
        assert_eq!(true, is_consistent(&credit));
        assert_eq!(100, credit.get_balance());
        assert_eq!(true, credit.realize_local_frozen_credits(100 + 10));
        assert_eq!(true, is_consistent(&credit));

        assert_eq!(-10, credit.get_balance());
    }

    #[test]
    fn test_consistency_while_fuzzing(){
        let mut credit = TokenChannelCredit::new(15, 15).unwrap();
        let mut rng = rand::thread_rng();
        let mut range = Range::new(0u64, 10u64);
        assert_eq!(true, is_consistent(&credit));
        for i in 0 .. 10000{
            if rng.gen(){
                credit.freeze_remote_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));
                credit.unfreeze_remote_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));
                credit.realize_remote_frozen_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));

            }else{
                credit.freeze_local_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));
                credit.unfreeze_local_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));
                credit.realize_local_frozen_credits(range.sample(&mut rng));
                assert_eq!(true, is_consistent(&credit));

            }
//            println!("{}", credit.get_balance());
        }

    }
}


