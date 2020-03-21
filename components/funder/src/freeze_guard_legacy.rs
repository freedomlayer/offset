/*
use std::collections::HashMap as ImHashMap;

use num_bigint::BigUint;

use crypto::identity::PublicKey;
use crypto::hash::{HashResult, sha_512_256};

use common::int_convert::usize_to_u32;
use signature::canonical::CanonicalSerialize;

use proto::funder::messages::{Ratio, FriendsRoute, FreezeLink};

use crate::credit_calc::CreditCalculator;
use crate::state::FunderState;
use crate::friend::ChannelStatus;


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
    // Total amount of credits frozen from A to B through this Offset node.
    // ```
    // A --> ... --> X --> B
    // ```
    // A could be any node, B must be a friend of this Offset node.
    frozen_to: ImHashMap<PublicKey, FriendFreezeGuard>,
    //                         ^ B
}

#[derive(Debug)]
pub enum FreezeGuardMutation {
    AddFrozenCredit((FriendsRoute, u128)), // (friends_route, dest_payment)
    SubFrozenCredit((FriendsRoute, u128)), // (friends_route, dest_payment)
}

fn hash_subroute(subroute: &[PublicKey]) -> HashResult {
    let mut hash_buffer = Vec::new();

    for public_key in subroute {
        hash_buffer.extend_from_slice(&public_key);
    }
    sha_512_256(&hash_buffer)
}

impl FreezeGuard {
    pub fn new(local_public_key: &PublicKey) -> FreezeGuard {
        FreezeGuard {
            local_public_key: local_public_key.clone(),
            frozen_to: ImHashMap::new(),
        }
    }

    pub fn mutate(&mut self, mutation: &FreezeGuardMutation) {
        match mutation {
            FreezeGuardMutation::AddFrozenCredit((friends_route, dest_payment)) =>
                self.add_frozen_credit(friends_route, *dest_payment),
            FreezeGuardMutation::SubFrozenCredit((friends_route, dest_payment)) =>
                self.sub_frozen_credit(friends_route, *dest_payment),
        }
    }

    // TODO: Should be moved outside of this structure implementation.
    // The only public function that allows mutation FreezeGuard should be mutate.
    pub fn load_funder_state<A>(mut self, funder_state: &FunderState<A>) -> FreezeGuard
    where
        A: CanonicalSerialize + Clone,
    {
        // Local public key should match:
        assert_eq!(self.local_public_key, funder_state.local_public_key);
        for (_friend_public_key, friend) in &funder_state.friends {
            if let ChannelStatus::Consistent(token_channel) = &friend.channel_status {
                let pending_local_requests = &token_channel
                    .get_mutual_credit()
                    .state()
                    .pending_requests
                    .pending_local_requests;
                for (_request_id, pending_request) in pending_local_requests {
                    self.add_frozen_credit(&pending_request.route, pending_request.dest_payment);
                }
            }
        }
        self
    }
    // TODO: Possibly refactor similar code of add/sub frozen credit to be one function that
    // returns an iterator?
    /// ```text
    /// A -- ... -- X -- B
    /// ```
    /// On the image: X is the local public key, B is a direct friend of X.
    /// For every node Y along the route, we add the credits Y needs to freeze because of the route
    /// from A to B.
    fn add_frozen_credit(&mut self, route: &FriendsRoute, dest_payment: u128) {
        assert!(route.public_keys.contains(&self.local_public_key));
        if &self.local_public_key == route.public_keys.last().unwrap() {
            // We are the destination. Nothing to do here.
            return;
        }

        let my_index = route.pk_to_index(&self.local_public_key).unwrap();

        let route_len = usize_to_u32(route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len, dest_payment);

        let next_index = my_index.checked_add(1).unwrap();
        let next_public_key = route.index_to_pk(next_index).unwrap().clone();
        let friend_freeze_guard = self.frozen_to
            .entry(next_public_key)
            .or_insert_with(FriendFreezeGuard::new);

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 ..= my_index {
            let node_public_key = route.index_to_pk(node_index).unwrap();

            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let routes_map = friend_freeze_guard
                .frozen_credits_from
                .entry(node_public_key.clone())
                .or_insert_with(ImHashMap::new);

            let route_hash = hash_subroute(&route.public_keys[node_index ..= next_index]);
            let route_entry = routes_map
                .entry(route_hash.clone())
                .or_insert(0);

            *route_entry = (*route_entry).checked_add(credits_to_freeze).unwrap();
        }
    }

    /// ```text
    /// A -- ... -- X -- B
    /// ```
    /// On the image: X is the local public key, B is a direct friend of X.
    /// For every node Y along the route, we subtract the credits Y needs to freeze because of the
    /// route from A to B. In other words, this method unfreezes frozen credits along this route.
    fn sub_frozen_credit(&mut self, route: &FriendsRoute, dest_payment: u128) {
        assert!(route.public_keys.contains(&self.local_public_key));
        if &self.local_public_key == route.public_keys.last().unwrap() {
            // We are the destination. Nothing to do here.
            return;
        }

        let my_index = route.pk_to_index(&self.local_public_key).unwrap();

        let route_len = usize_to_u32(route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len, dest_payment);

        let next_index = my_index.checked_add(1).unwrap();
        let next_public_key = route
            .index_to_pk(next_index).unwrap().clone();
        let friend_freeze_guard = self.frozen_to
            .get_mut(&next_public_key)
            .unwrap();

        // Iterate over all nodes from the beginning of the route until our index:
        for node_index in 0 ..= my_index {
            let node_public_key = route
                .index_to_pk(node_index)
                .unwrap();

            let next_node_index = usize_to_u32(node_index.checked_add(1).unwrap()).unwrap();
            let credits_to_freeze = credit_calc.credits_to_freeze(next_node_index).unwrap();
            let routes_map = friend_freeze_guard
                .frozen_credits_from
                .get_mut(&node_public_key)
                .unwrap();

            let route_hash = hash_subroute(&route.public_keys[node_index ..= next_index]);
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

    /// Get the amount of credits frozen from <from_pk> to <to_pk> going through this sub-route,
    /// where <to_pk> is a friend of this Offset node.
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

    /// ```text
    /// A -- ... -- X -- B
    /// ```
    /// X is the local public key. B is a direct friend of X.
    /// For any node Y along the route from A to X, we make sure that B does not freeze too many
    /// credits for this node. In other words, we save Y from freezing too many credits for B.
    pub fn verify_freezing_links(&self, route: &FriendsRoute,
                                 dest_payment: u128,
                                 freeze_links: &[FreezeLink]) -> Option<()> {
        assert!(freeze_links.len().checked_add(1).unwrap() <= route.len());
        let my_index = route.pk_to_index(&self.local_public_key).unwrap();
        // TODO: Do we ever get here as the destination of the route?
        let next_index = my_index.checked_add(1).unwrap();
        assert_eq!(next_index, freeze_links.len());

        let route_len = usize_to_u32(route.len()).unwrap();
        let credit_calc = CreditCalculator::new(route_len, dest_payment);

        let two_pow_128 = BigUint::new(vec![0x0u32, 0x0u32, 0x0u32, 0x0u32, 0x1u32]);
        assert_eq!(two_pow_128, BigUint::from(0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffffu128) + BigUint::from(1u128));

        // Verify previous freezing links
        for node_findex in 0 .. freeze_links.len() {
            let first_freeze_link = &freeze_links[node_findex];
            let mut allowed_credits: BigUint = first_freeze_link.shared_credits.into();
            for freeze_link in &freeze_links[
                node_findex .. freeze_links.len()] {

                allowed_credits = match freeze_link.usable_ratio {
                    Ratio::One => allowed_credits,
                    Ratio::Numerator(num) => {
                        let b_num = BigUint::from(num);
                        allowed_credits * b_num / &two_pow_128
                    },
                };
            }

            let subroute = &route
                .public_keys[node_findex ..= next_index];
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


#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::{PublicKey, PublicKey::len()};

    /// Get the amount of credits to be frozen on a route of a certain length
    /// with certain amount to pay.
    /// index is the location of the next node. For example, index = 1 will return the amount of
    /// credits node 0 should freeze.
    fn credit_freeze(route_len: u32, dest_payment: u128, index: u32) -> u128 {
        CreditCalculator::new(route_len, dest_payment).credits_to_freeze(index).unwrap()
    }

    #[test]
    fn test_get_frozen_basic() {
        /*
         * a -- b -- (c) -- d
        */

        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);
        let pk_d = PublicKey::from(&[0xdd; PublicKey::len()]);

        let mut freeze_guard = FreezeGuard::new(&pk_c);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 19);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,19,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,19,2), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,19,3), guard_frozen);

        freeze_guard.sub_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 19);
        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(guard_frozen, 0);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(guard_frozen, 0);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(guard_frozen, 0);
    }

    #[test]
    fn test_get_frozen_multiple_requests() {
        /*
         * a -- b -- (c) -- d
        */

        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);
        let pk_d = PublicKey::from(&[0xdd; PublicKey::len()]);

        let mut freeze_guard = FreezeGuard::new(&pk_c);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 11);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 17);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,2) + credit_freeze(3,17,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,3) + credit_freeze(3,17,2), guard_frozen);

        freeze_guard.sub_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 11);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(3,17,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(3,17,2), guard_frozen);

        freeze_guard.sub_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 17);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);
    }

    #[test]
    fn test_get_frozen_branch_out() {
        /*
         * a -- b -- (c) -- d
         *      |
         *      e
        */

        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);
        let pk_d = PublicKey::from(&[0xdd; PublicKey::len()]);
        let pk_e = PublicKey::from(&[0xee; PublicKey::len()]);

        let mut freeze_guard = FreezeGuard::new(&pk_c);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 11);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_e.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 17);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_e.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,17,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,2) + credit_freeze(4,17,2), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,3) + credit_freeze(4,17,3), guard_frozen);

        freeze_guard.sub_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_e.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 17);

        let guard_frozen = freeze_guard.get_frozen(&[pk_e.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,1), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,2), guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(credit_freeze(4,11,3), guard_frozen);

        freeze_guard.sub_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 11);

        let guard_frozen = freeze_guard.get_frozen(&[pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);

        let guard_frozen = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        assert_eq!(0, guard_frozen);
    }

    #[test]
    fn test_verify_freezing_links_basic() {
        /*
         * a -- b -- (c) -- d
         *      |
         *      e
        */

        let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_c = PublicKey::from(&[0xcc; PublicKey::len()]);
        let pk_d = PublicKey::from(&[0xdd; PublicKey::len()]);
        let pk_e = PublicKey::from(&[0xee; PublicKey::len()]);

        let mut freeze_guard = FreezeGuard::new(&pk_c);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_a.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 11);
        freeze_guard.add_frozen_credit(&FriendsRoute
                                       {public_keys: vec![pk_e.clone(), pk_b.clone(), pk_c.clone(), pk_d.clone()]}, 17);

        let frozen_b = freeze_guard.get_frozen(&[pk_b.clone(), pk_c.clone(), pk_d.clone()]);
        let frozen_c = freeze_guard.get_frozen(&[pk_c.clone(), pk_d.clone()]);
        let half = Ratio::Numerator(0x8000_0000_0000_0000_0000_0000_0000_0000);


        // -- Freezing not allowed, c -- d
        let route = FriendsRoute {public_keys: vec![pk_c.clone(), pk_d.clone()]};
        let freeze_link_c = FreezeLink {
            shared_credits: (frozen_c + credit_freeze(2,9,1) - 1)* 2,
            usable_ratio: half.clone(), // Half
        };
        let freeze_links = vec![freeze_link_c];
        let res = freeze_guard.verify_freezing_links(&route, 9, &freeze_links);
        assert!(res.is_none());


        // -- Freezing allowed: c -- d
        let route = FriendsRoute {public_keys: vec![pk_c.clone(), pk_d.clone()]};
        let freeze_link_c = FreezeLink {
            shared_credits: (frozen_c + credit_freeze(2,9,1)) * 2,
            usable_ratio: half.clone(), // Half
        };
        let freeze_links = vec![freeze_link_c];
        let res = freeze_guard.verify_freezing_links(&route, 9, &freeze_links);
        assert!(res.is_some());


        // -- Freezing not allowed: b -- c -- d
        let route = FriendsRoute {public_keys: vec![pk_b.clone(), pk_c.clone(), pk_d.clone()]};
        let freeze_link_b = FreezeLink {
            shared_credits: (frozen_b + credit_freeze(3,9,1) - 1) * 4,  // <-- should be not enough
            usable_ratio: half.clone(), // Half
        };
        let freeze_link_c = FreezeLink {
            shared_credits: (frozen_c + credit_freeze(3,9,2)) * 2,
            usable_ratio: half.clone(), // Half
        };
        let freeze_links = vec![freeze_link_b, freeze_link_c];
        let res = freeze_guard.verify_freezing_links(&route, 9, &freeze_links);
        assert!(res.is_none());

        // -- Freezing not allowed: b -- c -- d
        let route = FriendsRoute {public_keys: vec![pk_b.clone(), pk_c.clone(), pk_d.clone()]};
        let freeze_link_b = FreezeLink {
            shared_credits: (frozen_b + credit_freeze(3,9,1)) * 4,
            usable_ratio: half.clone(), // Half
        };
        let freeze_link_c = FreezeLink {
            shared_credits: (frozen_c + credit_freeze(3,9,2) - 1) * 2,  // <-- should be not enough
            usable_ratio: half.clone(), // Half
        };
        let freeze_links = vec![freeze_link_b, freeze_link_c];
        let res = freeze_guard.verify_freezing_links(&route, 9, &freeze_links);
        assert!(res.is_none());


        // -- Freezing allowed: b -- c -- d
        let route = FriendsRoute {public_keys: vec![pk_b.clone(), pk_c.clone(), pk_d.clone()]};
        let freeze_link_b = FreezeLink {
            shared_credits: (frozen_b + credit_freeze(3,9,1)) * 4,
            usable_ratio: half.clone(), // Half
        };
        let freeze_link_c = FreezeLink {
            shared_credits: (frozen_c + credit_freeze(3,9,2)) * 2,
            usable_ratio: half.clone(), // Half
        };
        let freeze_links = vec![freeze_link_b, freeze_link_c];
        let res = freeze_guard.verify_freezing_links(&route, 9, &freeze_links);
        assert!(res.is_some());
    }
}
*/
