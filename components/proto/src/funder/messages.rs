use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashSet;

use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;

use crypto::hash::{self, HashResult};
use crypto::hash_lock::{HashedLock, PlainLock};
use crypto::identity::{PublicKey, Signature};
use crypto::invoice_id::InvoiceId;
use crypto::payment_id::PaymentId;
use crypto::rand::RandValue;
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
    pub response_hash: HashResult,
    pub dest_payment: u128,
    pub src_plain_lock: PlainLock,
    pub dest_hashed_lock: HashedLock,
    pub signature: Signature,
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct MultiCommit {
    pub invoice_id: InvoiceId,
    pub total_dest_payment: u128,
    pub commits: Vec<Commit>,
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
    // = sha512/256(requestId || randNonce)
    pub invoice_id: InvoiceId,
    pub src_plain_lock: PlainLock,
    pub dest_plain_lock: PlainLock,
    pub dest_payment: u128,
    pub total_dest_payment: u128,
    pub signature: Signature,
    /*
    # Signature{key=destinationKey}(
    #   sha512/256("FUNDS_RESPONSE") ||
    #   sha512/256(requestId || sha512/256(route) || randNonce) ||
    #   srcHashedLock ||
    #   dstHashedLock ||
    #   destPayment ||
    #   totalDestPayment ||
    #   invoiceId
    # )
    */
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub enum TransactionStage {
    Request,
    Response(HashedLock), // inner: dest_hashed_lock.
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

    /// Check if no element repeats twice in the array
    fn is_unique(array: &[PublicKey]) -> bool {
        let mut seen = HashSet::new();
        for item in array {
            if !seen.insert(item.clone()) {
                return false;
            }
        }
        true
    }

    /// Check if the route is valid.
    /// A valid route must have at least 2 unique nodes, and is in one of the following forms:
    /// A -- B -- C -- D -- E -- F -- A   (Single cycle, first == last)
    /// A -- B -- C -- D -- E -- F        (A route with no repetitions)
    pub fn is_valid(&self) -> bool {
        if self.public_keys.len() < 2 {
            return false;
        }
        if self.public_keys.len() > MAX_ROUTE_LEN {
            return false;
        }

        let last_key = &self.public_keys[self.public_keys.len() - 1];
        if last_key == &self.public_keys[0] {
            // We have a first == last cycle.
            if self.public_keys.len() > 2 {
                // We have a cycle that is long enough (no A -- A).
                // We just check if it's a single cycle.
                Self::is_unique(&self.public_keys[1..])
            } else {
                // A -- A
                false
            }
        } else {
            // No first == last cycle.
            // But we have to check if there is any other cycle.
            Self::is_unique(&self.public_keys)
        }
    }

    /// Checks if the remaining part of the route is valid.
    /// Compared to regular version, this one does not check for minimal unique
    /// nodes amount. It returns `true` if the part is empty.
    /// It does not accept routes parts with a cycle, though.
    pub fn is_valid_part(&self) -> bool {
        if self.public_keys.len() > MAX_ROUTE_LEN - 1 {
            return false;
        }

        Self::is_unique(&self.public_keys)
    }

    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        hash::sha_512_256(&self.canonical_serialize())
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

    /// Calculate the amount of additional fee credits we have to pay if
    /// we want to pay `dest_payment` credits.
    pub fn calc_fee(&self, dest_payment: u128) -> Option<u128> {
        let mul_res = (BigUint::from(dest_payment) * BigUint::from(self.mul)) >> 32;
        let res = mul_res + BigUint::from(self.add);
        res.to_u128()
    }

    /// Maximum amount of credits we should be able to pay
    /// through a given capacity.
    ///
    /// Solves the equation:
    /// x + (mx + n) <= c
    /// As:
    /// x <= (c - n) / (m + 1)
    /// When m = m0 / 2^32, we get:
    /// x <= ((c - n) * 2^32) / (m0 + 2^32)
    pub fn max_payable(&self, capacity: u128) -> u128 {
        let long_add = u128::from(self.add);
        let c_minus_n = if let Some(c_minus_n) = capacity.checked_sub(long_add) {
            c_minus_n
        } else {
            // Right hand side is going to be non-positive, this means maximum payable is 0.
            return 0;
        };

        let numerator = BigUint::from(c_minus_n) << 32;
        let denominator = BigUint::from(self.mul) + (BigUint::from(1u128) << 32);
        (numerator / denominator).to_u128().unwrap()
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
    pub invoice_id: InvoiceId,
    /// Total amount of credits to be paid.
    pub total_dest_payment: u128,
}

/// Start an invoice (A request for payment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckClosePayment {
    pub payment_id: PaymentId,
    pub ack_uid: Uid,
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
    AckClosePayment(AckClosePayment),
    // Seller API:
    AddInvoice(AddInvoice),
    CancelInvoice(InvoiceId),
    CommitInvoice(MultiCommit),
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentStatus {
    PaymentNotFound,
    InProgress,              // Can not be acked
    Success((Receipt, Uid)), // (Receipt, ack_id)
    Canceled(Uid),           // ack_id
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseClosePayment {
    pub payment_id: PaymentId,
    pub status: PaymentStatus,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum FunderOutgoingControl<B: Clone> {
    TransactionResult(TransactionResult),
    ResponseClosePayment(ResponseClosePayment),
    ReportMutations(FunderReportMutations<B>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    #[test]
    fn test_friends_route_is_valid() {
        // Helper macro modeled after vec![].
        macro_rules! route {
            ( $($num:expr),* ) => {
                FriendsRoute {
                    public_keys: vec![
                        $(PublicKey::from(
                            &[$num; PUBLIC_KEY_LEN]
                        )),*
                    ],
                }
            }
        }

        assert_eq!(route![1].is_valid(), false); // too short
        assert_eq!(route![1].is_valid_part(), true); // long enough
        assert_eq!(route![].is_valid(), false); // too short
        assert_eq!(route![].is_valid_part(), true); // long enough

        // Test cases taken from https://github.com/freedomlayer/offst/pull/215#discussion_r292327613
        assert_eq!(route![1, 2, 3, 4].is_valid(), true); // usual route
        assert_eq!(route![1, 2, 3, 4, 1].is_valid(), true); // cyclic route that is at least 3 nodes long, having first item equal the last item
        assert_eq!(route![1, 1].is_valid(), false); // cyclic route that is too short (only 2 nodes long)
        assert_eq!(route![1, 2, 3, 2, 4].is_valid(), false); // Should have no repetitions that are not the first and last nodes.

        assert_eq!(route![1, 2, 3, 4].is_valid_part(), true); // usual route
        assert_eq!(route![1, 2, 3, 4, 1].is_valid_part(), false); // should have no cycles in a partial route
        assert_eq!(route![1, 1].is_valid_part(), false); // should have no repetitions ins a partial route
        assert_eq!(route![1, 2, 3, 2, 4].is_valid_part(), false); // should have no repetitions in a partial route
    }
}
