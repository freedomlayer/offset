use std::collections::hash_set::HashSet;

use byteorder::{WriteBytesExt, BigEndian};

use utils::int_convert::usize_to_u64;

use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::hash;
use crypto::hash::HashResult;

use proto::common::SendFundsReceipt;
use proto::funder::InvoiceId;
use proto::networker::{NetworkerSendPrice, ChannelToken};

#[derive(Clone)]
pub enum NeighborTcOp {
    EnableRequests(NetworkerSendPrice),
    DisableRequests,
    SetRemoteMaxDebt(u64),
    SetInvoiceId(InvoiceId),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage(RequestSendMessage),
    ResponseSendMessage(ResponseSendMessage),
    FailureSendMessage(FailureSendMessage),
    // ResetChannel(i64), // new_balanace
}

#[derive(Clone)]
pub struct PendingNeighborRequest {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content_hash: HashResult,
    pub request_content_len: u32,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
}


#[derive(Clone)]
pub struct ResponseSendMessage {
    pub request_id: Uid,
    pub rand_nonce: RandValue,
    pub processing_fee_collected: u64,
    pub response_content: Vec<u8>,
    pub signature: Signature,
}


/// A rational number. 
/// T is the type of the numerator and the denominator.
#[derive(Clone)]
pub struct Rational<T> {
    pub numerator: T,
    pub denominator: T,
}

#[derive(Clone)]
pub struct NetworkerFreezeLink {
    pub shared_credits: u64,
    pub usable_ratio: Rational<u64>,
}

#[derive(Clone)]
pub struct RequestSendMessage {
    pub request_id: Uid,
    pub route: NeighborsRoute,
    pub request_content: Vec<u8>,
    pub max_response_len: u32,
    pub processing_fee_proposal: u64,
    pub freeze_links: Vec<NetworkerFreezeLink>,
}

#[derive(Clone)]
pub struct RandNonceSignature {
    pub rand_nonce: RandValue,
    pub signature: Signature,
}


#[derive(Clone)]
pub struct FailureSendMessage {
    pub request_id: Uid,
    pub reporting_public_key: PublicKey,
    pub rand_nonce_signatures: Vec<RandNonceSignature>,
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
    pub (super) source_public_key: PublicKey,
    pub (super) source_request_proposal: NetworkerSendPrice,
    pub (super) route_links: Vec<NeighborRouteLink>,
    pub (super) dest_public_key: PublicKey,
    pub (super) dest_response_proposal: NetworkerSendPrice,
}


#[allow(unused)]
#[derive(Clone)]
pub struct NeighborMoveToken {
    pub token_channel_index: u16,
    pub operations: Vec<NeighborTcOp>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
    pub new_token: ChannelToken,
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

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.source_public_key);
        res_bytes.extend_from_slice(&self.source_request_proposal.to_bytes());

        let mut route_links_bytes = Vec::new();
        res_bytes.write_u64::<BigEndian>(
            usize_to_u64(self.route_links.len()).unwrap()).unwrap();
        for route_link in &self.route_links {
            route_links_bytes.extend_from_slice(&route_link.to_bytes());
        }
        res_bytes.extend_from_slice(&hash::sha_512_256(&route_links_bytes));

        res_bytes.extend_from_slice(&self.dest_public_key);
        res_bytes.extend_from_slice(&self.dest_response_proposal.to_bytes());
        res_bytes
    }

    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        hash::sha_512_256(&self.to_bytes())
    }

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

    /// Get the public key of a node according to its index.
    pub fn pk_by_index(&self, index: usize) -> Option<&PublicKey> {
        if index == 0 {
            Some(&self.source_public_key)
        } else if index == self.route_links.len().checked_add(2)? {
            Some(&self.dest_public_key)
        } else {
            let link_index = index.checked_sub(1)?;
            Some(&self.route_links[link_index].node_public_key)
        }
    }
}

impl RandNonceSignature {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

impl Rational<u64> {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u64::<BigEndian>(self.numerator)
            .expect("Could not serialize u64!");
        res_bytes.write_u64::<BigEndian>(self.denominator)
            .expect("Could not serialize u64!");
        res_bytes
    }
}

impl NetworkerFreezeLink {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u64::<BigEndian>(self.shared_credits)
            .expect("Could not serialize u64!");
        res_bytes.extend_from_slice(&self.usable_ratio.to_bytes());
        res_bytes
    }
}

impl RequestSendMessage {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.route.to_bytes());
        res_bytes.extend_from_slice(&self.request_content);
        res_bytes.write_u32::<BigEndian>(self.max_response_len)
            .expect("Failed to serialize max_response_len (u32)");
        res_bytes.write_u64::<BigEndian>(self.processing_fee_proposal)
            .expect("Failed to serialize processing_fee_proposal (u64)");
        for freeze_link in &self.freeze_links {
            res_bytes.extend_from_slice(&freeze_link.to_bytes());
        }
        res_bytes
    }
}

impl ResponseSendMessage {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.write_u64::<BigEndian>(self.processing_fee_collected)
            .expect("Failed to serialize processing_fee_collected (u64)");
        res_bytes.write_u64::<BigEndian>(usize_to_u64(
                self.response_content.len()).unwrap()).unwrap();
        res_bytes.extend_from_slice(&self.response_content);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}


impl FailureSendMessage {
    fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.reporting_public_key);
        res_bytes.write_u64::<BigEndian>(usize_to_u64(
                self.rand_nonce_signatures.len()).unwrap()).unwrap();
        for rand_nonce_sig in &self.rand_nonce_signatures {
            res_bytes.extend_from_slice(&rand_nonce_sig.to_bytes());
        }
        res_bytes
    }
}

#[allow(unused)]
impl NeighborTcOp {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            NeighborTcOp::EnableRequests(networker_send_price) => {
                res_bytes.push(0u8);
                res_bytes.append(&mut networker_send_price.to_bytes());
            },
            NeighborTcOp::DisableRequests => {
                res_bytes.push(1u8);
            }
            NeighborTcOp::SetRemoteMaxDebt(remote_max_debt) => {
                res_bytes.push(2u8);
                res_bytes.write_u64::<BigEndian>(*remote_max_debt)
                    .expect("Failed to serialize u64 (remote_max_debt)");
            }
            NeighborTcOp::SetInvoiceId(invoice_id) => {
                res_bytes.push(3u8);
                res_bytes.extend_from_slice(&invoice_id)
            }
            NeighborTcOp::LoadFunds(send_funds_receipt) => {
                res_bytes.push(4u8);
                res_bytes.append(&mut send_funds_receipt.to_bytes())
            }
            NeighborTcOp::RequestSendMessage(request_send_message) => {
                res_bytes.push(5u8);
                res_bytes.append(&mut request_send_message.to_bytes())
            }
            NeighborTcOp::ResponseSendMessage(response_send_message) => {
                res_bytes.push(6u8);
                res_bytes.append(&mut response_send_message.to_bytes())
            }
            NeighborTcOp::FailureSendMessage(failure_send_message) => {
                res_bytes.push(7u8);
                res_bytes.append(&mut failure_send_message.to_bytes())
            }
            
        }
        res_bytes
    }
}
