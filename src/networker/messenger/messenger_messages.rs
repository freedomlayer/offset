use crypto::identity::{PublicKey, verify_signature, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use std::mem;
use crypto::hash;
use crypto::hash::HashResult;
use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use proto::indexer::NeighborsRoute;
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

