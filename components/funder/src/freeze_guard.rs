use im::hashmap::HashMap as ImHashMap;

use num_bigint::BigUint;

use crypto::identity::PublicKey;
use crypto::hash::{HashResult, sha_512_256};
use utils::int_convert::usize_to_u32;

use super::credit_calc::CreditCalculator;
use super::state::FunderState;
use super::friend::ChannelStatus;
use super::types::{PendingFriendRequest, RequestSendFunds, Ratio};


#[derive(Clone)]
struct FriendFreezeGuard {
    frozen_credits_from: ImHashMap<PublicKey, ImHashMap<HashResult, u128>>
    //                              ^-A                 ^-hash(route)  ^-frozen
}

impl FriendFreezeGuard {
    fn new() -> FriendFreezeGuard {
        FriendFreezeGuard {
            frozen_credits_from: ImHashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct FreezeGuard {
    local_public_key: PublicKey,
    // Total amount of credits frozen from A to B through this CSwitch node.
    // ```
    // A --> ... --> X --> B
    // ```
    // A could be any node, B must be a friend of this CSwitch node.
    frozen_to: ImHashMap<PublicKey, FriendFreezeGuard>,
    //                         ^ B                 
}

fn hash_subroute(subroute: &[PublicKey]) -> HashResult {
    let mut hash_buffer = Vec::new();

    for public_key in subroute {
        hash_buffer.extend_from_slice(&public_key);
    }
    sha_512_256(&hash_buffer)
}

impl FreezeGuard {
    pub fn new<A: Clone>(funder_state: &FunderState<A>) -> FreezeGuard {
        let mut freeze_guard = FreezeGuard {
            local_public_key: funder_state.local_public_key.clone(),
            frozen_to: ImHashMap::new(),
        };

        for (_friend_public_key, friend) in &funder_state.friends {
            if let ChannelStatus::Consistent(directional) = &friend.channel_status {
                let pending_local_requests = &directional.token_channel.state().pending_requests.pending_local_requests;
                for (_request_id, pending_request) in pending_local_requests {
                    freeze_guard.add_frozen_credit(&pending_request);
                }
            }
        }

        freeze_guard
    }
    // TODO: Possibly refactor similar code of add/sub frozen credit to be one function that
    // returns an iterator?
    /// ```text
    /// A -- ... -- X -- B
    /// ```
    /// Add credits frozen by B of all nodes until us on the route.
    pub fn add_frozen_credit(&mut self, pending_request: &PendingFriendRequest) {
        if &self.local_public_key == pending_request.route.public_keys.last().unwrap() {
            // We are the destination. Nothing to do here.
            return;
        }

        let my_index = pending_request.route.pk_to_index(&self.local_public_key).unwrap();

        let route_len = usize_to_u32(pending_request.route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len,
                                                pending_request.dest_payment);

        let next_index = my_index.checked_add(1).unwrap();
        let next_public_key = pending_request.route
            .index_to_pk(next_index).unwrap().clone();
        let friend_freeze_guard = self.frozen_to
            .entry(next_public_key)
            .or_insert_with(FriendFreezeGuard::new);

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 .. my_index {
            let node_public_key = pending_request.route
                .index_to_pk(node_index)
                .unwrap();
            
            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let routes_map = friend_freeze_guard
                .frozen_credits_from
                .entry(node_public_key.clone())
                .or_insert_with(ImHashMap::new);

            let route_hash = hash_subroute(&pending_request.route.public_keys[node_index .. next_index]);
            let route_entry = routes_map
                .entry(route_hash.clone())
                .or_insert(0);

            *route_entry = (*route_entry).checked_add(credits_to_freeze).unwrap();
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

        let next_index = my_index.checked_add(1).unwrap();
        let next_public_key = pending_request.route
            .index_to_pk(next_index).unwrap().clone();
        let friend_freeze_guard = self.frozen_to
            .get_mut(&next_public_key)
            .unwrap();

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 .. my_index {
            let node_public_key = pending_request.route
                .index_to_pk(node_index)
                .unwrap();
            
            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let routes_map = friend_freeze_guard
                .frozen_credits_from
                .get_mut(&node_public_key)
                .unwrap();

            let route_hash = hash_subroute(&pending_request.route.public_keys[node_index .. next_index]);
            let route_entry = routes_map
                .get_mut(&route_hash)
                .unwrap();

            *route_entry = (*route_entry).checked_sub(credits_to_freeze).unwrap();
            // Cleanup:
            if *route_entry == 0 {
                routes_map.remove(&route_hash);
            }

            // Cleanup:
            if routes_map.is_empty() {
                friend_freeze_guard.frozen_credits_from.remove(&node_public_key);
            }
        }

        // Cleanup:
        if friend_freeze_guard.frozen_credits_from.is_empty() {
            self.frozen_to.remove(&next_public_key);
        }
    }

    /// Get the amount of credits frozen from <from_pk> to <to_pk> going through this CSwitch node,
    /// where <to_pk> is a friend of this CSwitch node.
    fn get_frozen(&self, subroute: &[PublicKey]) -> u128 {
        if subroute.len() < 2 {
            unreachable!();
        }
        let from_pk = &subroute[0];
        let to_pk = &subroute[subroute.len().checked_sub(1).unwrap()];

        self.frozen_to.get(to_pk)
            .and_then(|ref friend_freeze_guard| 
                 friend_freeze_guard.frozen_credits_from.get(from_pk))
            .and_then(|ref routes_map|
                 routes_map.get(&hash_subroute(subroute)).cloned())
            .unwrap_or(0u128)
    }

    pub fn verify_freezing_links(&self, request_send_funds: &RequestSendFunds) -> Option<()> {
        let my_index = request_send_funds.route.pk_to_index(&self.local_public_key).unwrap();
        // TODO: Do we ever get here as the destination of the request_send_funds?
        let next_index = my_index.checked_add(1).unwrap();
        assert_eq!(next_index, request_send_funds.freeze_links.len());

        let route_len = usize_to_u32(request_send_funds.route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len,
                                                request_send_funds.dest_payment);

        let two_pow_128 = BigUint::new(vec![0x1u32, 0x0u32, 0x0u32, 0x0u32, 0x0u32]);

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
            
            let subroute = &request_send_funds.route
                .public_keys[node_findex .. next_index];
            let old_frozen = self.get_frozen(subroute);

            let node_findex = usize_to_u32(node_findex).unwrap();
            let next_node_findex = node_findex.checked_add(1).unwrap();
            let new_frozen = credit_calc.credits_to_freeze(next_node_findex)?
                .checked_add(old_frozen).unwrap();
            if allowed_credits < new_frozen.into() {
                return None;
            }
        }
        Some(())
    }
}

