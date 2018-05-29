use std::mem;
use std::collections::HashSet;
use crypto::identity::{PublicKey, Signature};
use proto::networker::NetworkerSendPrice;

pub const INDEXING_PROVIDER_ID_LEN: usize = 16;
pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;

/// The identifier of an indexing provider.
define_fixed_bytes!(IndexingProviderId, INDEXING_PROVIDER_ID_LEN);

/// A hash of a full link in an indexing provider chain
define_fixed_bytes!(IndexingProviderStateHash, INDEXING_PROVIDER_STATE_HASH_LEN);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentProposalPair {
    pub request: NetworkerSendPrice,
    pub response: NetworkerSendPrice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborRouteLink {
    pub node_public_key: PublicKey,
    pub payment_proposal_pair: PaymentProposalPair,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborsRoute {
    source_public_key: PublicKey,
    source_request_proposal: NetworkerSendPrice,
    pub route_links: Vec<NeighborRouteLink>,
    dest_public_key: PublicKey,
    dest_response_proposal: NetworkerSendPrice,
}



#[derive(PartialEq, Eq)]
pub enum PkPairPosition {
    /// Requested pair is the last on the route.
    /// Included here the payment proposed for response.
    /// If a None is included, it means that default price is used.
    Dest(Option<NetworkerSendPrice>),
    /// Requested pair is not the last on the route.
    NotDest {
        next_public_key: PublicKey,
        request_payment_proposal: NetworkerSendPrice,
        opt_response_payment_proposal: Option<NetworkerSendPrice>,
    },
}

impl NeighborRouteLink {
    pub fn bytes_count() -> usize {
        mem::size_of::<PublicKey>() + 
            NetworkerSendPrice::bytes_count() * 2
    }
}

impl NeighborsRoute {
    pub fn bytes_count(&self) -> usize {
        mem::size_of::<PublicKey>() * 2 +
            NeighborRouteLink::bytes_count()
                * self.route_links.len()
    }

    /// Check if every node shows up in the route at most once.
    /// This makes sure no cycles are present
    pub fn is_cycle_free(&self) -> bool {
        // All seen public keys:
        let mut seen = HashSet::new();
        if !seen.insert(self.source_public_key.clone()) { 
            return false
        }
        if !seen.insert(self.dest_public_key.clone()) {
            return false
        }
        for route_link in &self.route_links {
            if !seen.insert(route_link.node_public_key.clone()) {
                return false
            }
        }
        true
    }

    /// Find two consecutive public keys (pk1, pk2) inside a neighbors route.
    /// If found, returns information about payment proposals and next node's public key.
    pub fn find_pk_pair(&self, pk1: &PublicKey, pk2: &PublicKey) -> Option<PkPairPosition> {
        if self.route_links.is_empty() {
            if &self.source_public_key == pk1 && &self.dest_public_key == pk2 {
                Some(PkPairPosition::Dest(None))
            } else {
                None
            }
        } else {
            let rl = &self.route_links;
            if &self.source_public_key == pk1 && &rl[0].node_public_key == pk2 {
                Some(PkPairPosition::NotDest {
                    next_public_key: rl[0].node_public_key.clone(),
                    request_payment_proposal: 
                        rl[0].payment_proposal_pair.request.clone(),
                    opt_response_payment_proposal: None,
                })
            } else if &rl[rl.len() - 1].node_public_key == pk1 && &self.dest_public_key == pk2 {
                Some(PkPairPosition::Dest(Some(rl[rl.len() - 1]
                                               .payment_proposal_pair
                                               .request.clone())))
            } else {
                for i in 1 .. rl.len() {
                    if &rl[i-1].node_public_key == pk1 && &rl[i].node_public_key == pk2 {
                        return Some(PkPairPosition::NotDest {
                            next_public_key: rl[i].node_public_key.clone(),
                            request_payment_proposal: rl[i]
                                .payment_proposal_pair
                                .request.clone(),
                            opt_response_payment_proposal: Some(rl[i-1]
                                                                .payment_proposal_pair
                                                                .response.clone()),
                        })
                    }
                }
                None
            }
        }
    }
}
/*

    pub fn is_unique(&self) -> bool {
        let public_keys = &self.public_keys;
        for i in 0 .. public_keys.len() {
            for j in i + 1.. public_keys.len() {
                if public_keys[i] == public_keys[j]{
                    return false
                }
            }
        }
        true
    }

    fn index_of(&self, key: &PublicKey) -> Option<usize>{
        self.public_keys.iter().position(|k| k == key)
    }

    pub fn get_dest_public_key(&self) -> Option<PublicKey>{
        let key = self.public_keys.last()?;
        Some(key.clone())
    }

    pub fn distance_between_nodes(&self, first_node: &PublicKey, second_node: &PublicKey)
        -> Option<usize>{
        let index_first = self.index_of(first_node)?;
        let index_second = self.index_of(second_node)?;
        if index_first > index_second{
            None
        }else{
            Some(index_second - index_first)
        }
    }
}
*/

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexerRoute {
    pub neighbors_route: NeighborsRoute,
    pub app_port: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriendsRouteWithCapacity {
    pub public_keys: Vec<PublicKey>,
    // How much credit can we push through this route?
    pub capacity: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateChainLink {
    pub previous_state_hash: IndexingProviderStateHash,
    pub new_owners_public_keys: Vec<PublicKey>,
    pub new_indexers_public_keys: Vec<PublicKey>,
    pub signatures_by_old_owners: Vec<Signature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestNeighborsRoutes {
    pub source_node_public_key: PublicKey,
    pub destination_node_public_key: PublicKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseNeighborsRoutes {
    pub routes: Vec<NeighborsRoute>,
}

#[allow(unused)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestFriendsRoutes {
    Direct {
        source_node_public_key: PublicKey,
        destination_node_public_key: PublicKey,
    },
    LoopFromFriend {
        // A loop from myself through given friend, back to myself.
        // This is used for money rebalance when we owe the friend money.
        friend_public_key: PublicKey,
    },
    LoopToFriend {
        // A loop from myself back to myself through given friend.
        // This is used for money rebalance when the friend owe us money.
        friend_public_key: PublicKey,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseFriendsRoutes {
    pub routes: Vec<FriendsRouteWithCapacity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseUpdateState {
    pub state_hash: IndexingProviderStateHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestUpdateState {
    pub indexing_provider_id: IndexingProviderId,
    pub indexing_provider_states_chain: Vec<StateChainLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutesToIndexer {
    pub indexing_provider_id: IndexingProviderId,
    pub routes: Vec<IndexerRoute>,
    pub request_price: u64,
}
