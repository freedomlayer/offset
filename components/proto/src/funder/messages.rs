use std::collections::HashSet;
use byteorder::{WriteBytesExt, BigEndian};

use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crypto::uid::Uid;
use crypto::hash::{self, HashResult};

use common::int_convert::{usize_to_u64};
use common::canonical_serialize::CanonicalSerialize;

use crate::funder::report::FunderReportMutation;
use crate::consts::MAX_ROUTE_LEN;


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TPublicKey<P> {
    pub public_key: P,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TSignature<S> {
    pub signature: S,
}

impl<P> TPublicKey<P> {
    pub fn new(public_key: P) -> Self {
        TPublicKey {
            public_key,
        }
    }
}

impl<S> TSignature<S> {
    pub fn new(signature: S) -> Self {
        TSignature {
            signature,
        }
    }
}

impl<P> CanonicalSerialize for TPublicKey<P>
where
    P: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        self.public_key.canonical_serialize()
    }
}

impl<S> CanonicalSerialize for TSignature<S>
where
    S: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        self.signature.canonical_serialize()
    }
}

/*
impl<S> Default for TSignature<S> 
where
    S: Default,
{
    fn default() -> Self {
        TSignature {
            signature: S::default(),
        }
    }
}
*/


#[allow(unused)]
#[derive(Debug)]
pub enum FunderToChanneler<A,P> {
    /// Send a message to a friend
    Message((TPublicKey<P>, Vec<u8>)), // (friend_public_key, message)
    /// Set address for relay used by local node
    /// None means that no address is configured.
    SetAddress(Option<A>), 
    /// Request to add a new friend
    AddFriend((TPublicKey<P>, A)), // (friend_public_key, address)
    /// Request to remove a friend
    RemoveFriend(TPublicKey<P>), // friend_public_key
}

#[allow(unused)]
#[derive(Debug)]
pub enum ChannelerToFunder<P> {
    /// A friend is now online
    Online(TPublicKey<P>),
    /// A friend is now offline
    Offline(TPublicKey<P>),
    /// Incoming message from a remote friend
    Message((TPublicKey<P>, Vec<u8>)), // (friend_public_key, message)
}

// -------------------------------------------

/// The ratio can be numeration / T::max_value(), or 1
#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum Ratio<T> {
    One,
    Numerator(T),
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct FreezeLink {
    pub shared_credits: u128,
    pub usable_ratio: Ratio<u128>
}

pub const INVOICE_ID_LEN: usize = 32;

/// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendsRoute<P> {
    pub public_keys: Vec<TPublicKey<P>>,
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RequestSendFunds<P> {
    pub request_id: Uid,
    pub route: FriendsRoute<P>,
    pub dest_payment: u128,
    pub invoice_id: InvoiceId,
    pub freeze_links: Vec<FreezeLink>,
}


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSendFunds<PRS> {
    pub request_id: Uid,
    pub rand_nonce: RandValue,
    pub signature: PRS,
}

pub type SignedResponse<RS> = ResponseSendFunds<TSignature<RS>>;
pub type UnsignedResponse = ResponseSendFunds<()>;


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct FailureSendFunds<P,PFS> {
    pub request_id: Uid,
    pub reporting_public_key: TPublicKey<P>,
    pub rand_nonce: RandValue,
    pub signature: PFS,
}

pub type SignedFailure<P,FS> = FailureSendFunds<P,TSignature<FS>>;
pub type UnsignedFailure<P> = FailureSendFunds<P,()>;


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum FriendTcOp<P,RS,FS> {
    EnableRequests,
    DisableRequests,
    SetRemoteMaxDebt(u128),
    RequestSendFunds(RequestSendFunds<P>),
    ResponseSendFunds(SignedResponse<RS>),
    FailureSendFunds(SignedFailure<P,FS>),
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveToken<A,P,RS,FS,MS,PMS> {
    pub operations: Vec<FriendTcOp<P,RS,FS>>,
    pub opt_local_address: Option<A>,
    pub old_token: TSignature<MS>,
    pub local_public_key: TPublicKey<P>,
    pub remote_public_key: TPublicKey<P>,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: PMS,
}

pub type SignedMoveToken<A,P,RS,FS,MS> = MoveToken<A,P,RS,FS,MS,TSignature<MS>>;
pub type UnsignedMoveToken<A,P,RS,FS,MS> = MoveToken<A,P,RS,FS,MS,()>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetTerms<MS> {
    pub reset_token: TSignature<MS>,
    pub inconsistency_counter: u64,
    pub balance_for_reset: i128,
}

#[derive(PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub struct MoveTokenRequest<A,P,RS,FS,MS> {
    pub friend_move_token: SignedMoveToken<A,P,RS,FS,MS>,
    // Do we want the remote side to return the token:
    pub token_wanted: bool,
}


#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FriendMessage<A,P,RS,FS,MS> {
    MoveTokenRequest(MoveTokenRequest<A,P,RS,FS,MS>),
    InconsistencyError(ResetTerms<MS>),
}

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SendFundsReceipt<RS> {
    pub response_hash: HashResult,
    // = sha512/256(requestId || sha512/256(route) || randNonce)
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
    pub signature: TSignature<RS>,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(route) || randNonce) ||
    //   invoiceId ||
    //   destPayment
    // )
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct PendingRequest<P> {
    pub request_id: Uid,
    pub route: FriendsRoute<P>,
    pub dest_payment: u128,
    pub invoice_id: InvoiceId,
}



// ==================================================================
// ==================================================================

impl CanonicalSerialize for Ratio<u128> {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match *self {
            Ratio::One => {
                res_bytes.write_u8(0).unwrap();
            },
            Ratio::Numerator(num) => {
                res_bytes.write_u8(1).unwrap();
                res_bytes.write_u128::<BigEndian>(num).unwrap();
            },
        }
        res_bytes
    }
}

impl CanonicalSerialize for FreezeLink {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u128::<BigEndian>(self.shared_credits).unwrap();
        res_bytes.extend_from_slice(&self.usable_ratio.canonical_serialize());
        res_bytes
    }
}

impl<P> CanonicalSerialize for RequestSendFunds<P> 
where
    P: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.route.canonical_serialize());
        res_bytes.write_u128::<BigEndian>(self.dest_payment).unwrap();
        for freeze_link in &self.freeze_links {
            res_bytes.extend_from_slice(&freeze_link.canonical_serialize());
        }
        res_bytes
    }
}

impl<RS> CanonicalSerialize for ResponseSendFunds<RS> 
where
    RS: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature.canonical_serialize());
        res_bytes
    }
}


impl<P,RS> CanonicalSerialize for FailureSendFunds<P,RS> 
where
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.reporting_public_key.canonical_serialize());
        res_bytes.extend_from_slice(&self.signature.canonical_serialize());
        res_bytes
    }
}

impl<P,RS,FS> CanonicalSerialize for FriendTcOp<P,RS,FS> 
where
    P: CanonicalSerialize,
    RS: CanonicalSerialize,
    FS: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            FriendTcOp::EnableRequests => {
                res_bytes.push(0u8);
            },
            FriendTcOp::DisableRequests => {
                res_bytes.push(1u8);
            }
            FriendTcOp::SetRemoteMaxDebt(remote_max_debt) => {
                res_bytes.push(2u8);
                res_bytes.write_u128::<BigEndian>(*remote_max_debt).unwrap();
            }
            FriendTcOp::RequestSendFunds(request_send_funds) => {
                res_bytes.push(3u8);
                res_bytes.append(&mut request_send_funds.canonical_serialize())
            }
            FriendTcOp::ResponseSendFunds(response_send_funds) => {
                res_bytes.push(4u8);
                res_bytes.append(&mut response_send_funds.canonical_serialize())
            }
            FriendTcOp::FailureSendFunds(failure_send_funds) => {
                res_bytes.push(5u8);
                res_bytes.append(&mut failure_send_funds.canonical_serialize())
            }
            
        }
        res_bytes
    }
}


impl<P> CanonicalSerialize for FriendsRoute<P> 
where
    P: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.write_u64::<BigEndian>(
            usize_to_u64(self.public_keys.len()).unwrap()).unwrap();
        for public_key in &self.public_keys {
            res_bytes.extend_from_slice(&public_key.canonical_serialize());
        }
        res_bytes
    }
}

impl<P> FriendsRoute<P> 
where
    P: CanonicalSerialize + Eq + std::hash::Hash,
{
    pub fn len(&self) -> usize {
        self.public_keys.len()
    }

    /// Check if the route is valid.
    /// A valid route must have at least 2 nodes, and is in one of the following forms:
    /// A -- B -- C -- D -- E -- F -- A   (Single cycle, first == last)
    /// A -- B -- C -- D -- E -- F        (A route with no repetitions)
    pub fn is_valid(&self) -> bool {
        if (self.public_keys.len() < 2) || (self.public_keys.len() > MAX_ROUTE_LEN) {
            return false;
        }

        let mut seen = HashSet::new();
        for public_key in &self.public_keys[.. self.public_keys.len() - 1] {
            if !seen.insert(public_key.clone()) {
                return false;
            }
        }
        let last_pk = &self.public_keys[self.public_keys.len() - 1];
        if last_pk == &self.public_keys[0] {
            // The last public key closes a cycle
            true
        } else {
            // A route with no repetitions
            seen.insert(last_pk.clone())
        }
    }

    /// Find two consecutive public keys (pk1, pk2) inside a friends route.
    pub fn find_pk_pair(&self, pk1: &TPublicKey<P>, pk2: &TPublicKey<P>) -> Option<usize> {
        let pks = &self.public_keys;
        for i in 0 ..= pks.len().checked_sub(2)? {
            if pk1 == &pks[i] && pk2 == &pks[i+1] {
                return Some(i);
            }
        }
        None
    }

    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        hash::sha_512_256(&self.canonical_serialize())
    }

    /// Find the index of a public key inside the route.
    /// source is considered to be index 0.
    /// dest is considered to be the last index.
    ///
    /// Note that the returned index does not map directly to an 
    /// index of self.route_links vector.
    pub fn pk_to_index(&self, public_key: &TPublicKey<P>) -> Option<usize> {
        self.public_keys
            .iter()
            .position(|cur_public_key| cur_public_key == public_key)
    }

    /// Get the public key of a node according to its index.
    pub fn index_to_pk(&self, index: usize) -> Option<&TPublicKey<P>> {
        self.public_keys.get(index)
    }
}


impl<RS> CanonicalSerialize for SendFundsReceipt<RS> 
where
    RS: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes.write_u128::<BigEndian>(self.dest_payment).unwrap();
        res_bytes.extend_from_slice(&self.signature.canonical_serialize());
        res_bytes
    }
}

// AppServer <-> Funder communication:
// ===================================

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatus {
    Enabled = 1,
    Disabled = 0,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatus {
    Open,
    Closed,
}

impl RequestsStatus {
    pub fn is_open(&self) -> bool {
        if let RequestsStatus::Open = self {
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddFriend<A,P> {
    pub friend_public_key: TPublicKey<P>,
    pub address: A,
    pub name: String,
    pub balance: i128, // Initial balance
}

#[derive(Debug, Clone)]
pub struct RemoveFriend<P> {
    pub friend_public_key: TPublicKey<P>,
}

#[derive(Debug, Clone)]
pub struct SetRequestsStatus<P> {
    pub friend_public_key: TPublicKey<P>,
    pub status: RequestsStatus,
}

#[derive(Debug, Clone)]
pub struct SetFriendStatus<P> {
    pub friend_public_key: TPublicKey<P>,
    pub status: FriendStatus,
}

#[derive(Debug, Clone)]
pub struct SetFriendRemoteMaxDebt<P> {
    pub friend_public_key: TPublicKey<P>,
    pub remote_max_debt: u128,
}

#[derive(Debug, Clone)]
pub struct SetFriendName<P> {
    pub friend_public_key: TPublicKey<P>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SetFriendAddress<A,P> {
    pub friend_public_key: TPublicKey<P>,
    pub address: A,
}

#[derive(Debug, Clone)]
pub struct ResetFriendChannel<P,MS> {
    pub friend_public_key: TPublicKey<P>,
    pub current_token: TSignature<MS>,
}

/// A request to send funds that originates from the user
#[derive(Debug, Clone)]
pub struct UserRequestSendFunds<P> {
    pub request_id: Uid,
    pub route: FriendsRoute<P>,
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
}


#[derive(Debug, Clone)]
pub struct ReceiptAck<RS> {
    pub request_id: Uid,
    pub receipt_signature: TSignature<RS>,
}

#[derive(Debug, Clone)]
pub enum FunderIncomingControl<A,P,RS,MS> {
    /// Set relay address used for the local node
    SetAddress(Option<A>),
    AddFriend(AddFriend<A,P>),
    RemoveFriend(RemoveFriend<P>),
    SetRequestsStatus(SetRequestsStatus<P>),
    SetFriendStatus(SetFriendStatus<P>),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt<P>),
    SetFriendAddress(SetFriendAddress<A,P>),
    SetFriendName(SetFriendName<P>),
    ResetFriendChannel(ResetFriendChannel<P,MS>),
    RequestSendFunds(UserRequestSendFunds<P>),
    ReceiptAck(ReceiptAck<RS>),
}

impl<P> UserRequestSendFunds<P> 
where
    P: Clone,
{
    pub fn to_request(self) -> RequestSendFunds<P> {
        RequestSendFunds {
            request_id: self.request_id,
            route: self.route,
            invoice_id: self.invoice_id,
            dest_payment: self.dest_payment,
            freeze_links: Vec::new(),
        }
    }

    pub fn create_pending_request(&self) -> PendingRequest<P> {
        PendingRequest {
            request_id: self.request_id,
            route: self.route.clone(),
            dest_payment: self.dest_payment,
            invoice_id: self.invoice_id.clone(),
        }
    }
}

#[derive(Debug)]
pub enum ResponseSendFundsResult<P,RS> {
    Success(SendFundsReceipt<RS>),
    Failure(TPublicKey<P>), // Reporting public key.
}


#[derive(Debug)]
pub struct ResponseReceived<P,RS> {
    pub request_id: Uid,
    pub result: ResponseSendFundsResult<P,RS>,
}


#[derive(Debug)]
pub enum FunderOutgoingControl<A: Clone,P,RS,MS> {
    ResponseReceived(ResponseReceived<P,RS>),
    // Report(FunderReport<A>),
    ReportMutations(Vec<FunderReportMutation<A,P,MS>>),
}

// ------------------------------------------
// ------------------------------------------


impl<A,P,RS,FS,MS> SignedMoveToken<A,P,RS,FS,MS>
where
    MS: Default,
{
    pub fn initial(local_public_key: TPublicKey<P>, 
                   remote_public_key: TPublicKey<P>,
                   balance: i128) -> Self {

        MoveToken {
            operations: Vec::new(),
            opt_local_address: None,
            old_token: TSignature::<MS>::default(),
            local_public_key,
            remote_public_key,
            inconsistency_counter: 0, 
            move_token_counter: 0,
            balance,
            local_pending_debt: 0,
            remote_pending_debt: 0,
            rand_nonce: RandValue::from(&[0; RAND_VALUE_LEN]),
            new_token: TSignature::<MS>::default(),
        }
    }
}
