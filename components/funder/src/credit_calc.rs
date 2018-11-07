#![warn(unused)]

use std::convert::TryFrom;
// use utils::int_convert::usize_to_u32;

// TODO: Why do we take node_index and route_len as u32? 
// Possibly change this in the future?

/// Amount of credit paid to a node that sent a valid Response
///
/// ```text
///    B  --  (C)  --   D   --   E   --   F   
/// ```
///
/// In the example above, num_nodes = 5, node_index = 1 for the node C.
/// Upon any overflow (u128) this function will return None.
///
pub fn credits_on_success(node_index: u32, 
                          route_len: u32,
                          dest_payment: u128) -> Option<u128> {
    if node_index == 0 {
        None
    } else {
        let dist = route_len.checked_sub(node_index)?.checked_sub(1)?;
        u128::try_from(dist)?.checked_add(dest_payment)
    }
}


/// The amount of credits paid to a node in case of failure.
///
pub fn credits_on_failure() -> Option<u128> {
    Some(0)
}


/// Compute the amount of credits we need to freeze when sending a request to a node which is
/// nodes_to_dest nodes from the destination.
///
/// ```text
///                           req      req      req
///                           res      res      res      res
///                    B  --   C   --  (D)   --   E   --   F   
///
/// node_index:        0       1        2         3        4
///
/// In the above example, if we plan to send a message from C to D, 
/// we should have node_index = 2 in order to calculate the amount of credits C should freeze.
/// ```
///
pub fn credits_to_freeze(node_index: u32, 
                         route_len: u32,
                         dest_payment: u128) -> Option<u128> {
    credits_on_success(node_index, route_len, dest_payment)
}


/// A credit calculator object that is wired to work with a specific request.
pub struct CreditCalculator {
    route_len: u32,
    dest_payment: u128,
}

impl CreditCalculator {
    pub fn new(route_len: u32,
               dest_payment: u128) -> Self {

        CreditCalculator {
            route_len,
            dest_payment
        }
    }

    /// Amount of credits node <index-1> should freeze when sending 
    /// a request message to node <index>
    /// Source node has index 0. Destination node has index route_len - 1.
    pub fn credits_to_freeze(&self, node_index: u32) -> Option<u128> {
        credits_to_freeze(node_index, self.route_len, self.dest_payment)
    }

    /// Amount of credits to be paid to node <index> when it sends a valid response to node
    /// <index-1>
    /// Source node has index 0. Destination node has index route_len - 1.
    pub fn credits_on_success(&self, node_index: u32) -> Option<u128> {
        credits_on_success(node_index, self.route_len, self.dest_payment)
    }

    /// Amount of credits to be paid to node <index> when it sends a failure message to node
    /// <index-1>
    /// Source node has index 0. Destination node has index route_len - 1.
    pub fn credits_on_failure(&self, _node_index: u32, _reporting_node_index: u32) -> Option<u128> {
        credits_on_failure()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    // use num_traits::PrimInt;
    // use std::cmp;


    // TODO: Add tests for CreditCalculator.

    /*
    fn is_linear<F,N,M>(f: F, begin: N, end: N) -> bool
    where 
        F: Fn(N) -> M,
        N: PrimInt,
        M: PrimInt,
    {

        let n_one = N::one();
        let n_two = n_one + n_one;
        assert!(end >= begin + n_two);
        let mut x = begin;
        while x < end - n_two {
            if f(x + n_two) + f(x) != f(x + n_one) + f(x + n_one) {
                return false
            }
            x = x + n_one;
        }
        true
    }
    */

    #[test]
    fn test_credits_on_success_monotone() {
        let route_len = 40;
        let dest_payment = 100;
        let mut opt_old_credits = None;
        // Zero index doesn't get anything:
        assert_eq!(credits_on_success(0, route_len, dest_payment), None);

        // First index is paid the most, and the last node on the route gets the least. Payment is
        // telescopic:
        for node_index in 1 .. route_len {
            let credits = credits_on_success(node_index, route_len, dest_payment);
            match opt_old_credits {
                None => {},
                Some(old_credits) => {assert!(old_credits > credits)},
            };
            opt_old_credits = Some(credits);
        }
    }

    #[test]
    fn test_credits_to_freeze_bigger_than_success() {
        let route_len = 40;
        let dest_payment = 100;

        // First index is paid the most, and the last node on the route gets the least. Payment is
        // telescopic:
        for node_index in 1 .. route_len {
            let success_credits = credits_on_success(node_index, route_len, dest_payment);
            let freeze_credits = credits_to_freeze(node_index, route_len, dest_payment);
            assert!(freeze_credits >= success_credits);
        }
    }
}

