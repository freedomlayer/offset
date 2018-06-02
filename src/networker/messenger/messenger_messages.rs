use std::mem;
use std::collections::hash_set::HashSet;
use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;
use crypto::hash::HashResult;
use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use proto::common::SendFundsReceipt;
use proto::funder::InvoiceId;
use proto::networker::NetworkerSendPrice;
use super::pending_neighbor_request::PendingNeighborRequest;


pub enum NeighborTcOp {
    EnableRequests(NetworkerSendPrice),
    DisableRequests,
    SetRemoteMaxDebt(u64),
    SetInvoiceId(InvoiceId),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage(RequestSendMessage),
    ResponseSendMessage(ResponseSendMessage),
    FailedSendMessage(FailedSendMessage),
    // ResetChannel(i64), // new_balanace
}


pub struct ResponseSendMessage {
    pub request_id: Uid,
    pub rand_nonce: RandValue,
    pub processing_fee_collected: u64,
    pub response_content: Vec<u8>,
    pub signature: Signature,
}


/// A rational number. 
/// T is the type of the numerator and the denominator.
pub struct Rational<T> {
    pub numerator: T,
    pub denominator: T,
}

pub struct NetworkerFreezeLink {
    pub shared_credits: u64,
    pub usable_ratio: Rational<u64>,
}

pub struct RequestSendMessage {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content: Vec<u8>,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
    pub freeze_links: Vec<NetworkerFreezeLink>,
}


// TODO: Update this structure to contain updated contents of FailedSendMessage.
pub struct FailedSendMessage {
    request_id: Uid,
    reporting_public_key: PublicKey,
    rand_nonce: RandValue,
    signature: Signature,
}

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
