use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashSet;

use crypto::crypto_rand::RandValue;
use crypto::hash::{self, HashResult};
use crypto::hash_lock::{HashedLock, PlainLock};
use crypto::identity::{PublicKey, Signature};
use crypto::invoice_id::InvoiceId;
use crypto::payment_id::PaymentId;
use crypto::uid::Uid;

use crate::app_server::messages::{NamedRelayAddress, RelayAddress};
use crate::consts::MAX_ROUTE_LEN;
use crate::net::messages::NetAddress;
use crate::report::messages::FunderReportMutations;
use common::canonical_serialize::CanonicalSerialize;
use common::int_convert::usize_to_u64;

#[derive(Debug, Clone)]
pub struct ChannelerUpdateFriend<RA> {
    pub friend_public_key: PublicKey,
    /// We should try to connect to this address:
    pub friend_relays: Vec<RA>,
    /// We should be listening on this address:
    pub local_relays: Vec<RA>,
}

#[derive(Debug)]
pub enum FunderToChanneler<RA> {
    /// Send a message to a friend
    Message((PublicKey, Vec<u8>)), // (friend_public_key, message)
    /// Set address for relay used by local node
    SetRelays(Vec<RA>),
    /// Request to add a new friend or update friend's information
    UpdateFriend(ChannelerUpdateFriend<RA>),
    /// Request to remove a friend
    RemoveFriend(PublicKey), // friend_public_key
}

#[derive(Debug)]
pub enum ChannelerToFunder {
    /// A friend is now online
    Online(PublicKey),
    /// A friend is now offline
    Offline(PublicKey),
    /// Incoming message from a remote friend
    Message((PublicKey, Vec<u8>)), // (friend_public_key, message)
}

// -------------------------------------------

/*
pub const INVOICE_ID_LEN: usize = 32;

// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);
*/

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendsRoute {
    pub public_keys: Vec<PublicKey>,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RequestSendFundsOp {
    pub request_id: Uid,
    pub src_hashed_lock: HashedLock,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub total_dest_payment: u128,
    pub invoice_id: InvoiceId,
    pub left_fees: u128,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSendFundsOp<S = Signature> {
    pub request_id: Uid,
    pub dest_hashed_lock: HashedLock,
    pub rand_nonce: RandValue,
    pub signature: S,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct CancelSendFundsOp {
    pub request_id: Uid,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    response_hash: HashResult,
    dest_payment: u128,
    src_plain_lock: PlainLock,
    dest_hashed_lock: HashedLock,
    signature: Signature,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct MultiCommit {
    invoice_id: InvoiceId,
    total_dest_payment: u128,
    commits: Vec<Commit>,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct CollectSendFundsOp {
    pub request_id: Uid,
    pub src_plain_lock: PlainLock,
    pub dest_plain_lock: PlainLock,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum FriendTcOp {
    EnableRequests,
    DisableRequests,
    SetRemoteMaxDebt(u128),
    RequestSendFunds(RequestSendFundsOp),
    ResponseSendFunds(ResponseSendFundsOp),
    CancelSendFunds(CancelSendFundsOp),
    CollectSendFunds(CollectSendFundsOp),
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveToken<B = NetAddress, S = Signature> {
    pub operations: Vec<FriendTcOp>,
    pub opt_local_relays: Option<Vec<RelayAddress<B>>>,
    pub old_token: Signature,
    pub local_public_key: PublicKey,
    pub remote_public_key: PublicKey,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: S,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetTerms {
    pub reset_token: Signature,
    pub inconsistency_counter: u64,
    pub balance_for_reset: i128,
}

#[derive(PartialEq, Eq, Clone, Serialize, Debug)]
pub struct MoveTokenRequest<B = NetAddress> {
    pub friend_move_token: MoveToken<B>,
    // Do we want the remote side to return the token:
    pub token_wanted: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FriendMessage<B = NetAddress> {
    MoveTokenRequest(MoveTokenRequest<B>),
    InconsistencyError(ResetTerms),
}

/// A `Receipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub response_hash: HashResult,
    // = sha512/256(requestId || sha512/256(route) || randNonce)
    pub invoice_id: InvoiceId,
    pub src_plain_lock: PlainLock,
    pub dest_plain_lock: PlainLock,
    pub dest_payment: u128,
    pub total_dest_payment: u128,
    pub signature: Signature,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(route) || randNonce) ||
    //   invoiceId ||
    //   destPayment
    // )
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub enum TransactionStage {
    Request,
    Response(HashedLock), // inner: dest_hashed_lock. TOOD: Change to use bcrypt
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub total_dest_payment: u128,
    pub invoice_id: InvoiceId,
    pub left_fees: u128,
    pub src_hashed_lock: HashedLock,
    pub stage: TransactionStage,
}

// ==================================================================
// ==================================================================

impl CanonicalSerialize for RequestSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.src_hashed_lock);
        res_bytes.extend_from_slice(&self.route.canonical_serialize());
        res_bytes
            .write_u128::<BigEndian>(self.dest_payment)
            .unwrap();
        res_bytes.extend_from_slice(&self.invoice_id);
        // We do not sign over`left_fees`, because this field changes as the request message is
        // forwarded.
        // res_bytes.write_u128::<BigEndian>(self.left_fees).unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for ResponseSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.dest_hashed_lock);
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

impl CanonicalSerialize for CancelSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes
    }
}

impl CanonicalSerialize for CollectSendFundsOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.request_id);
        res_bytes.extend_from_slice(&self.src_plain_lock);
        res_bytes.extend_from_slice(&self.dest_plain_lock);
        res_bytes
    }
}

impl CanonicalSerialize for FriendTcOp {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            FriendTcOp::EnableRequests => {
                res_bytes.push(0u8);
            }
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
            FriendTcOp::CancelSendFunds(cancel_send_funds) => {
                res_bytes.push(5u8);
                res_bytes.append(&mut cancel_send_funds.canonical_serialize())
            }
            FriendTcOp::CollectSendFunds(commit_send_funds) => {
                res_bytes.push(6u8);
                res_bytes.append(&mut commit_send_funds.canonical_serialize())
            }
        }
        res_bytes
    }
}

impl CanonicalSerialize for FriendsRoute {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes
            .write_u64::<BigEndian>(usize_to_u64(self.public_keys.len()).unwrap())
            .unwrap();
        for public_key in &self.public_keys {
            res_bytes.extend_from_slice(public_key);
        }
        res_bytes
    }
}

impl FriendsRoute {
    pub fn len(&self) -> usize {
        self.public_keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.public_keys.is_empty()
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
        for public_key in &self.public_keys[..self.public_keys.len() - 1] {
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
    pub fn find_pk_pair(&self, pk1: &PublicKey, pk2: &PublicKey) -> Option<usize> {
        let pks = &self.public_keys;
        for i in 0..=pks.len().checked_sub(2)? {
            if pk1 == &pks[i] && pk2 == &pks[i + 1] {
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
    pub fn pk_to_index(&self, public_key: &PublicKey) -> Option<usize> {
        self.public_keys
            .iter()
            .position(|cur_public_key| cur_public_key == public_key)
    }

    /// Get the public key of a node according to its index.
    pub fn index_to_pk(&self, index: usize) -> Option<&PublicKey> {
        self.public_keys.get(index)
    }
}

impl CanonicalSerialize for Receipt {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes
            .write_u128::<BigEndian>(self.dest_payment)
            .unwrap();
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}

// AppServer <-> Funder communication:
// ===================================

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatus {
    Enabled,
    Disabled,
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

/// Rates for forwarding a transaction
/// For a transaction of `x` credits, the amount of fees will be:
/// `(x * mul) / 2^32 + add`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rate {
    /// Commission
    pub mul: u32,
    /// Flat rate
    pub add: u32,
}

impl Rate {
    pub fn new() -> Self {
        Rate { mul: 0, add: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFriend<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress<B>>,
    pub name: String,
    pub balance: i128, // Initial balance
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveFriend {
    pub friend_public_key: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRequestsStatus {
    pub friend_public_key: PublicKey,
    pub status: RequestsStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendStatus {
    pub friend_public_key: PublicKey,
    pub status: FriendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendRemoteMaxDebt {
    pub friend_public_key: PublicKey,
    pub remote_max_debt: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendName {
    pub friend_public_key: PublicKey,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendRelays<B = NetAddress> {
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress<B>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetFriendChannel {
    pub friend_public_key: PublicKey,
    pub reset_token: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendRate {
    pub friend_public_key: PublicKey,
    pub rate: Rate,
}

/// A friend's route with known capacity
#[derive(Debug, Clone, PartialEq, Eq)]
struct FriendsRouteCapacity {
    route: FriendsRoute,
    capacity: u128,
}

/// A request to send funds that originates from the user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRequestSendFunds {
    pub payment_id: PaymentId,
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptAck {
    pub request_id: Uid,
    pub receipt_signature: Signature,
}

/// Start a payment, possibly by paying through multiple routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePayment {
    /// payment_id is a randomly generated value (by the user), allowing the user to refer to a
    /// certain payment.
    pub payment_id: PaymentId,
    pub invoice_id: InvoiceId,
    pub total_dest_payment: u128,
    pub dest_public_key: PublicKey,
}

/// Start a payment, possibly by paying through multiple routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTransaction {
    /// A payment id of an existing payment.
    pub payment_id: PaymentId,
    /// Randomly generated request_id (by the user),
    /// allows the user to refer to this request later.
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub dest_payment: u128,
    pub fees: u128,
}

/// Start an invoice (A request for payment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddInvoice {
    /// Randomly generated invoice_id, allows to refer to this invoice.
    invoice_id: InvoiceId,
    /// Total amount of credits to be paid.
    total_dest_payment: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderControl<B> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriend<B>),
    RemoveFriend(RemoveFriend),
    SetRequestsStatus(SetRequestsStatus),
    SetFriendStatus(SetFriendStatus),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    SetFriendRelays(SetFriendRelays<B>),
    SetFriendName(SetFriendName),
    SetFriendRate(SetFriendRate),
    ResetFriendChannel(ResetFriendChannel),
    // Buyer API:
    CreatePayment(CreatePayment),
    CreateTransaction(CreateTransaction), // TODO
    RequestClosePayment(PaymentId),
    AckClosePayment((PaymentId, Uid)), // (payment_id, ack_id)
    // Seller API:
    AddInvoice(InvoiceId),
    CancelInvoice(InvoiceId),
    CommitInvoice(InvoiceId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunderIncomingControl<B> {
    pub app_request_id: Uid,
    pub funder_control: FunderControl<B>,
}

impl<B> FunderIncomingControl<B> {
    pub fn new(app_request_id: Uid, funder_control: FunderControl<B>) -> Self {
        FunderIncomingControl {
            app_request_id,
            funder_control,
        }
    }
}

impl UserRequestSendFunds {
    /*
    pub fn into_request(self) -> RequestSendFunds {
        RequestSendFunds {
            request_id: self.request_id,
            route: self.route,
            invoice_id: self.invoice_id,
            dest_payment: self.dest_payment,
        }
    }

    pub fn create_pending_transaction(&self) -> PendingTransaction {
        PendingTransaction {
            request_id: self.request_id,
            route: self.route.clone(),
            dest_payment: self.dest_payment,
            invoice_id: self.invoice_id.clone(),
        }
    }
    */
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestResult {
    Success(Commit),
    // TODO: Should we add more information to the failure here?
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionResult {
    pub request_id: Uid,
    pub result: RequestResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseClosePayment {
    InProgress,              // Can not be acked
    Success((Receipt, Uid)), // (Receipt, ack_id)
    Canceled(Uid),           // ack_id
}

#[derive(Debug)]
pub enum FunderOutgoingControl<B: Clone> {
    TransactionResult(TransactionResult),
    ResponseClosePayment(ResponseClosePayment),
    ReportMutations(FunderReportMutations<B>),
}
