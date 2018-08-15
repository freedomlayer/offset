use std::collections::HashMap;

use num_bigint::BigUint;

use crypto::identity::PublicKey;
use utils::int_convert::usize_to_u32;

use super::credit_calc::CreditCalculator;
use super::types::{PendingFriendRequest, RequestSendFunds, Ratio};

pub struct FreezeGuard {
    local_public_key: PublicKey,
    // Total amount of credits frozen from A to B through this CSwitch node.
    // ```
    // A -- ... -- X -- B
    // ```
    // A could be any node, B must be a friend of this CSwitch node.
    total_frozen: HashMap<PublicKey, HashMap<PublicKey, u128>>,
}

impl FreezeGuard {
    // TODO: Possibly refactor similar code of add/sub frozen credit to be one function that
    // returns an iterator?
    /// ```text
    /// A -- ... -- X -- B
    /// ```
    /// Add credits frozen by B of all all nodes until us on the route.
    pub fn add_frozen_credit(&mut self, pending_request: &PendingFriendRequest) {
        if &self.local_public_key == pending_request.route.public_keys.last().unwrap() {
            // We are the destination. Nothing to do here.
            return;
        }

        let my_index = pending_request.route.pk_to_index(&self.local_public_key).unwrap();

        let route_len = usize_to_u32(pending_request.route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len,
                                                pending_request.dest_payment);


        let next_public_key = pending_request.route
            .index_to_pk(my_index.checked_add(1).unwrap()).unwrap().clone();
        let friend_map = self.total_frozen
            .entry(next_public_key)
            .or_insert_with(HashMap::new);

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 .. my_index {
            let node_public_key = pending_request.route
                .index_to_pk(node_index)
                .unwrap();
            
            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let entry = friend_map
                .entry(node_public_key.clone())
                .or_insert(0);
            *entry = (*entry).checked_add(credits_to_freeze).unwrap();
        }
    }

    pub fn sub_frozen_credit(&mut self, pending_request: &PendingFriendRequest) {
        if &self.local_public_key == pending_request.route.public_keys.last().unwrap() {
            // We are the destination. Nothing to do here.
            return;
        }

        let my_index = pending_request.route.pk_to_index(&self.local_public_key).unwrap();
        let route_len = usize_to_u32(pending_request.route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len,
                                                pending_request.dest_payment);


        let next_public_key = pending_request.route
            .index_to_pk(my_index.checked_add(1).unwrap()).unwrap().clone();
        let friend_map = self.total_frozen.get_mut(&next_public_key).unwrap();

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 .. my_index {
            let node_public_key = pending_request.route
                .index_to_pk(node_index)
                .unwrap();
            
            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let entry = friend_map.get_mut(&node_public_key).unwrap();
            *entry = (*entry).checked_sub(credits_to_freeze).unwrap();

            // Cleanup:
            if *entry == 0 {
                friend_map.remove(&node_public_key);
            }
        }
        // Cleanup:
        if friend_map.is_empty() {
            self.total_frozen.remove(&next_public_key);
        }
    }

    /// Get the amount of credits frozen from <from_pk> to <to_pk> going through this CSwitch node,
    /// where <to_pk> is a friend of this CSwitch node.
    pub fn get_frozen(&self, from_pk: &PublicKey, to_pk: &PublicKey) -> u128 {
        match self.total_frozen.get(to_pk) {
            None => 0,
            Some(friend_map) => {
                match friend_map.get(from_pk) {
                    None => 0,
                    Some(frozen_credits) => *frozen_credits
                }
            }
        }
    }

    pub fn verify_freezing_links(&self, request_send_funds: &RequestSendFunds) -> Option<()> {
        let my_index = request_send_funds.route.pk_to_index(&self.local_public_key).unwrap();
        // TODO: Do we ever get here as the destination of the request_send_funds?
        let next_public_key = request_send_funds.route
            .index_to_pk(my_index.checked_add(1).unwrap()).unwrap().clone();

        let route_len = usize_to_u32(request_send_funds.route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len,
                                                request_send_funds.dest_payment);


        let two_pow_128 = BigUint::new(vec![0x1, 0x0, 0x0, 0x0, 0x0]);

        // Verify previous freezing links
        #[allow(needless_range_loop)]
        for node_findex in 0 .. request_send_funds.freeze_links.len() {
            let first_freeze_link = &request_send_funds.freeze_links[node_findex];
            let mut allowed_credits: BigUint = first_freeze_link.shared_credits.into();
            for freeze_link in &request_send_funds.freeze_links[
                node_findex .. request_send_funds.freeze_links.len()] {
                
                allowed_credits = match freeze_link.usable_ratio {
                    Ratio::One => allowed_credits,
                    Ratio::Numerator(num) => {
                        let b_num: BigUint = num.into();
                        allowed_credits * b_num / &two_pow_128
                    },
                };
            }

            let old_frozen = self.get_frozen(
                &request_send_funds.route.index_to_pk(node_findex).unwrap(),
                &next_public_key);
            let node_findex = usize_to_u32(node_findex).unwrap();
            let new_frozen = credit_calc.credits_to_freeze(node_findex)?
                .checked_add(old_frozen).unwrap();
            if allowed_credits < new_frozen.into() {
                return None;
            }
        }
        Some(())
    }
}

