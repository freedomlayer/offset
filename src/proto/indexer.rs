// use std::mem;
use std::collections::HashSet;
use crypto::identity::{PublicKey, Signature};
use crypto::hash;
use crypto::hash::HashResult;
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
    pub dest_response_proposal: NetworkerSendPrice,
}



#[derive(PartialEq, Eq)]
pub enum PkPairPosition {
    /// Requested pair is the last on the route.
    Dest,
    /// Requested pair is not the last on the route.
    /// We return the index of the second PublicKey in the pair
    /// inside the route_links array.
    NotDest(usize), 
}

impl PaymentProposalPair {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request.to_bytes());
        res_bytes.extend_from_slice(&self.response.to_bytes());
        res_bytes
    }
}

impl NeighborRouteLink {
    /*
    pub fn bytes_count() -> usize {
        mem::size_of::<PublicKey>() + 
            NetworkerSendPrice::bytes_count() * 2
    }
    */

    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.node_public_key);
        res_bytes.extend_from_slice(&self.payment_proposal_pair.to_bytes());
        res_bytes
    }
}

impl NeighborsRoute {
    /*
    pub fn bytes_count(&self) -> usize {
        mem::size_of::<PublicKey>() * 2 +
            NeighborRouteLink::bytes_count()
                * self.route_links.len()
    }
    */

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
    pub fn find_pk_pair(&self, pk1: &PublicKey, pk2: &PublicKey) -> Option<PkPairPosition> {
        if self.route_links.is_empty() {
            if &self.source_public_key == pk1 && &self.dest_public_key == pk2 {
                Some(PkPairPosition::Dest)
            } else {
                None
            }
        } else {
            let rl = &self.route_links;
            if &self.source_public_key == pk1 && &rl[0].node_public_key == pk2 {
                Some(PkPairPosition::NotDest(0))
            } else if &rl[rl.len() - 1].node_public_key == pk1 && &self.dest_public_key == pk2 {
                Some(PkPairPosition::NotDest(rl.len() - 1))
            } else {
                for i in 1 .. rl.len() {
                    if &rl[i-1].node_public_key == pk1 && &rl[i].node_public_key == pk2 {
                        return Some(PkPairPosition::NotDest(i))
                    }
                }
                None
            }
        }
    }

    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        let mut hbuffer = Vec::new();
        hbuffer.extend_from_slice(&self.source_public_key);
        hbuffer.extend_from_slice(&self.source_request_proposal.to_bytes());

        let mut route_links_bytes = Vec::new();
        for route_link in &self.route_links {
            route_links_bytes.extend_from_slice(&route_link.to_bytes());
        }
        hbuffer.extend_from_slice(&hash::sha_512_256(&route_links_bytes));

        hbuffer.extend_from_slice(&self.dest_public_key);
        hbuffer.extend_from_slice(&self.dest_response_proposal.to_bytes());

        hash::sha_512_256(&hbuffer)
    }

    /*
    /// Find the index of a public key inside the route.
    /// source is considered to be index 0.
    /// dest is considered to be the last index.
    ///
    /// Note that the returned index does not map directly to an 
    /// index of self.route_links vector.
    pub fn pk_index(&self, pk: &PublicKey) -> Option<usize> {
        if self.source_public_key == *pk {
            Some(0usize)
        } else if self.dest_public_key == *pk {
            Some(self.route_links.len().checked_add(1)?)
        } else {
            self.route_links
                .iter()
                .map(|route_link| &route_link.node_public_key)
                .position(|node_public_key| *node_public_key == *pk)
                .map(|i| i.checked_add(1))?
        }
    }
    */
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
