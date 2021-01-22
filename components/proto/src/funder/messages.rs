use std::cmp::Eq;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::hash::Hash;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use derive_more::Display;

use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;

use crate::crypto::{
    HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PublicKey, Signature, Uid,
};

use crate::app_server::messages::{NamedRelayAddress, RelayAddress, RelayAddressPort};
use crate::consts::MAX_CURRENCY_LEN;
use crate::net::messages::NetAddress;

use common::ser_utils::{ser_b64, ser_string, ser_vec_b64};
use common::u256::U256;

// use crate::wrapper::Wrapper;

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
pub const InvoiceId::len(): usize = 32;

// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, InvoiceId::len());
*/

/*
#[derive(Arbitrary, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendsRoute {
    #[serde(with = "ser_vec_b64")]
    pub public_keys: Vec<PublicKey>,
}
*/

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RequestSendFundsOp {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    /// Currency used for this request
    #[serde(with = "ser_string")]
    pub currency: Currency,
    /// A hash lock created by the originator of this request
    #[serde(with = "ser_b64")]
    pub src_hashed_lock: HashedLock,
    /// Amount paid to destination
    pub dest_payment: u128,
    /// hash(hash(actionId) || hash(totalDestPayment) || hash(description) || hash(additional))
    /// TODO: Check if this scheme is safe? Do we need to use pbkdf instead?
    #[serde(with = "ser_b64")]
    pub invoice_hash: HashResult,
    /// List of next nodes to transfer this request
    #[serde(with = "ser_vec_b64")]
    pub route: Vec<PublicKey>,
    /// Amount of fees left to give to mediators
    /// Every mediator takes the amount of fees he wants and subtracts this
    /// value accordingly.
    #[serde(with = "ser_string")]
    pub left_fees: u128,
}

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSendFundsOp {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    #[serde(with = "ser_b64")]
    pub src_plain_lock: PlainLock,
    /// Serial number used for this collection of invoice money.
    /// This should be a u128 counter, increased by 1 for every collected
    /// invoice.
    #[serde(with = "ser_string")]
    pub serial_num: u128,
    /// Signature{key=destinationKey}(
    ///   hash("FUNDS_RESPONSE") ||
    ///   hash(request_id || src_plain_lock || dest_payment) ||
    ///   hash(currency) ||
    ///   serialNum ||
    ///   invoiceHash)
    /// )
    #[serde(with = "ser_b64")]
    pub signature: Signature,
}

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedResponseSendFundsOp {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    #[serde(with = "ser_b64")]
    pub src_plain_lock: PlainLock,
    /// Serial number used for this collection of invoice money.
    /// This should be a u128 counter, increased by 1 for every collected
    /// invoice.
    #[serde(with = "ser_string")]
    pub serial_num: u128,
}

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct CancelSendFundsOp {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
}

/*
#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    #[serde(with = "ser_b64")]
    pub response_hash: HashResult,
    #[serde(with = "ser_b64")]
    pub src_plain_lock: PlainLock,
    #[serde(with = "ser_b64")]
    pub dest_hashed_lock: HashedLock,
    #[serde(with = "ser_string")]
    pub dest_payment: u128,
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_b64")]
    pub signature: Signature,
}
*/

// TODO: Possibly shorten names inside enum?
#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum FriendTcOp {
    RequestSendFunds(RequestSendFundsOp),
    ResponseSendFunds(ResponseSendFundsOp),
    CancelSendFunds(CancelSendFundsOp),
}

/*
#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum OptLocalRelays<B = NetAddress> {
    Empty,
    Relays(Vec<RelayAddress<B>>),
}
*/

impl Into<UnsignedResponseSendFundsOp> for ResponseSendFundsOp {
    fn into(self) -> UnsignedResponseSendFundsOp {
        UnsignedResponseSendFundsOp {
            request_id: self.request_id,
            src_plain_lock: self.src_plain_lock,
            serial_num: self.serial_num,
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    #[serde(with = "ser_string")]
    pub balance: i128,
    /// Frozen credits by our side
    #[serde(with = "ser_string")]
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    #[serde(with = "ser_string")]
    pub remote_pending_debt: u128,
    // TODO: in_fees and out_fees should be u256, not u128!
    /// Fees that were received from remote side
    #[serde(with = "ser_string")]
    pub in_fees: U256,
    /// Fees that were given to remote side
    #[serde(with = "ser_string")]
    pub out_fees: U256,
}

impl McBalance {
    pub fn new(balance: i128, in_fees: U256, out_fees: U256) -> McBalance {
        McBalance {
            balance,
            local_pending_debt: 0,
            remote_pending_debt: 0,
            in_fees,
            out_fees,
        }
    }

    pub fn flip(self) -> Self {
        Self {
            balance: self.balance.checked_neg().unwrap(),
            local_pending_debt: self.remote_pending_debt,
            remote_pending_debt: self.local_pending_debt,
            in_fees: self.out_fees,
            out_fees: self.in_fees,
        }
    }
}

/// Implicit values that both sides agree upon.
/// Those values are also signed as part of the prefix hash.
/// A hash of this structure is included inside MoveToken.
#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Hash of a sorted list of all the balances
    pub balances_hash: HashResult,
    // pub balances: HashMap<Currency, MutualCreditInfo>,
    /// Current token counter. Used as an alternative form of monotone time
    #[serde(with = "ser_string")]
    pub move_token_counter: u128,
}

/*
#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CurrencyOperations {
    #[serde(with = "ser_string")]
    pub operations: Vec<FriendTcOp>,
}
*/

// pub type CurrenciesOperations = HashMap<Currency, Vec<FriendTcOp>>;
// pub type CurrenciesOperations = Vec<(Currency, FriendTcOp)>;

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveToken {
    #[serde(with = "ser_b64")]
    pub old_token: Signature,
    pub operations: Vec<FriendTcOp>,
    #[serde(with = "ser_b64")]
    pub new_token: Signature,
}

/*
#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct UnsignedMoveToken<B = NetAddress> {
    #[serde(with = "ser_b64")]
    pub old_token: Signature,
    pub currencies_operations: Vec<CurrencyOperations>,
    pub relays_diff: Vec<RelayAddress<B>>,
    pub currencies_diff: Vec<Currency>,
    #[serde(with = "ser_b64")]
    pub info_hash: HashResult,
}
*/

/*
impl<B> Into<UnsignedMoveToken<B>> for MoveToken<B> {
    fn into(self) -> UnsignedMoveToken<B> {
        UnsignedMoveToken {
            old_token: self.old_token,
            currencies_operations: self.currencies_operations,
            relays_diff: self.relays_diff,
            currencies_diff: self.currencies_diff,
            info_hash: self.info_hash,
        }
    }
}
*/

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
#[display(fmt = "{}", currency)]
pub struct Currency {
    currency: String,
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.currency)
    }
}

struct CurrencyVisitor;

impl<'de> Visitor<'de> for CurrencyVisitor {
    type Value = Currency;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Currency string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let currency = Currency::try_from(value.to_owned())
            .map_err(|e| E::custom(format!("Invalid Currency string {:?}: {}", e, value)))?;
        Ok(currency)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Currency, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CurrencyVisitor)
    }
}

impl quickcheck::Arbitrary for Currency {
    fn arbitrary<G: quickcheck::Gen>(g: &mut G) -> Currency {
        let size = rand::Rng::gen_range(g, 1, MAX_CURRENCY_LEN);
        let mut s = String::with_capacity(size);
        for _ in 0..size {
            let new_char = rand::seq::SliceRandom::choose(&['a', 'b', 'c', 'd'][..], g)
                .unwrap()
                .to_owned();
            s.push(new_char);
        }
        Currency { currency: s }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Currency>> {
        // Shrink a string by shrinking a vector of its characters.
        let chars: Vec<char> = self.currency.chars().collect();
        Box::new(chars.shrink().map(|x| Currency {
            currency: x.into_iter().collect::<String>(),
        }))
    }
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyBalance {
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub balance: i128,
}

/// Balances for resetting a currency
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetBalance {
    pub balance: i128,
    pub in_fees: U256,
    pub out_fees: U256,
}

// TODO: Maybe shouldn't be cloneable (because reset balances could be large)
// TODO: Might move to proto in the future:
/// Reset terms for a token channel
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetTerms {
    pub reset_token: Signature,
    pub move_token_counter: u128,
    // TODO: Rename:
    pub reset_balances: HashMap<Currency, ResetBalance>,
}

#[derive(Arbitrary, PartialEq, Eq, Clone, Serialize, Debug)]
pub struct MoveTokenRequest {
    pub move_token: MoveToken,
    // Do we want the remote side to return the token:
    pub token_wanted: bool,
}

#[derive(Arbitrary, PartialEq, Eq, Clone, Serialize, Debug)]
pub struct RelaysUpdate {
    pub generation: u128,
    pub relays: Vec<RelayAddressPort>,
}

#[allow(clippy::large_enum_variant)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FriendMessage {
    MoveTokenRequest(MoveTokenRequest),
    InconsistencyError(ResetTerms),
    RelaysUpdate(RelaysUpdate),
    RelaysAck(u128), // generation
}

/*
/// A `Receipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    #[serde(with = "ser_b64")]
    pub response_hash: HashResult,
    // = sha512/256(requestId || randNonce)
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    pub currency: Currency,
    #[serde(with = "ser_b64")]
    pub src_plain_lock: PlainLock,
    #[serde(with = "ser_b64")]
    pub dest_plain_lock: PlainLock,
    pub is_complete: bool,
    #[serde(with = "ser_string")]
    pub dest_payment: u128,
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    #[serde(with = "ser_b64")]
    pub signature: Signature,
    /*
    # Signature{key=destinationKey}(
    #   sha512/256("FUNDS_RESPONSE") ||
    #   sha512/256(requestId || sha512/256(route) || randNonce) ||
    #   srcHashedLock ||
    #   dstHashedLock ||
    #   isComplete ||       (Assumed to be True)
    #   destPayment ||
    #   totalDestPayment ||
    #   invoiceId ||
    #   currency
    # )
    */
}
*/

#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    /// Id number of this request. Used to identify the whole transaction
    /// over this route.
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    /// Currency used for this request
    #[serde(with = "ser_string")]
    pub currency: Currency,
    /// A hash lock created by the originator of this request
    #[serde(with = "ser_b64")]
    pub src_hashed_lock: HashedLock,
    /// Amount paid to destination
    pub dest_payment: u128,
    /// hash(hash(actionId) || hash(totalDestPayment) || hash(description) || hash(additional))
    /// TODO: Check if this scheme is safe? Do we need to use pbkdf instead?
    #[serde(with = "ser_b64")]
    pub invoice_hash: HashResult,
    /// List of next nodes to transfer this request
    #[serde(with = "ser_vec_b64")]
    pub route: Vec<PublicKey>,
    /// Amount of fees left to give to mediators
    /// Every mediator takes the amount of fees he wants and subtracts this
    /// value accordingly.
    #[serde(with = "ser_string")]
    pub left_fees: u128,
}

// ==================================================================
// ==================================================================

// TODO: Possibly impl FriendsRoute as a trait for Vec<PublicKey>?

/*
impl FriendsRoute {
    pub fn len(&self) -> usize {
        self.public_keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.public_keys.is_empty()
    }

    /*
    /// Produce a cryptographic hash over the contents of the route.
    pub fn hash(&self) -> HashResult {
        hash::sha_512_256(&self.canonical_serialize())
    }
    */

    /// Get the public key of a node according to its index.
    pub fn index_to_pk(&self, index: usize) -> Option<&PublicKey> {
        self.public_keys.get(index)
    }

    /// Check if the route (e.g. `FriendsRoute`) is valid.
    /// A valid route must have at least 2 unique nodes, and is in one of the following forms:
    /// A -- B -- C -- D -- E -- F -- A   (Single cycle, first == last)
    /// A -- B -- C -- D -- E -- F        (A route with no repetitions)
    pub fn is_valid(&self) -> bool {
        is_route_valid(&self)
    }

    /// Checks if the remaining part of the route (e.g. `FriendsRoute`) is valid.
    /// Compared to regular version, this one does not check for minimal unique
    /// nodes amount. It returns `true` if the part is empty.
    /// It does not accept routes parts with a cycle, though.
    pub fn is_part_valid(&self) -> bool {
        is_route_part_valid(&self)
    }
}

use std::ops::Deref;
/// This `Deref` lets us use `is_route_valid` over `FriendsRoute`
impl Deref for FriendsRoute {
    type Target = [PublicKey];
    fn deref(&self) -> &Self::Target {
        self.public_keys.as_ref()
    }
}

/// Check if no element repeats twice in the slice
fn no_duplicates<T: Hash + Eq>(array: &[T]) -> bool {
    let mut seen = HashSet::new();
    for item in array {
        if !seen.insert(item) {
            return false;
        }
    }
    true
}

fn is_route_valid<T: Hash + Eq>(route: &[T]) -> bool {
    if route.len() < 2 {
        return false;
    }
    if route.len() > MAX_ROUTE_LEN {
        return false;
    }

    // route.len() >= 2
    let last_key = route.last().unwrap();
    if last_key == &route[0] {
        // We have a first == last cycle.
        if route.len() > 2 {
            // We have a cycle that is long enough (no A -- A).
            // We just check if it's a single cycle.
            no_duplicates(&route[1..])
        } else {
            // A -- A
            false
        }
    } else {
        // No first == last cycle.
        // But we have to check if there is any other cycle.
        no_duplicates(&route)
    }
}

fn is_route_part_valid<T: Hash + Eq>(route: &[T]) -> bool {
    // Route part should not be full route.
    // TODO: ensure it never is.
    if route.len() >= MAX_ROUTE_LEN {
        return false;
    }

    no_duplicates(route)
}
*/

// AppServer <-> Funder communication:
// ===================================

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatus {
    Enabled,
    Disabled,
}

#[derive(Arbitrary, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
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
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFriend<B = NetAddress> {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress<B>>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveFriend {
    pub friend_public_key: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendCurrencyRequestsStatus {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
    pub status: RequestsStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetFriendStatus {
    pub friend_public_key: PublicKey,
    pub status: FriendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendCurrencyMaxDebt {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
    #[serde(with = "ser_string")]
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
pub struct SetFriendCurrencyRate {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
    pub rate: Rate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveFriendCurrency {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
}

/// A friend's route with known capacity
#[derive(Debug, Clone, PartialEq, Eq)]
struct FriendsRouteCapacity {
    route: Vec<PublicKey>,
    capacity: u128,
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
    pub currency: Currency,
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
    pub route: Vec<PublicKey>,
    pub dest_payment: u128,
    pub fees: u128,
}

/// Start an invoice (A request for payment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddInvoice {
    /// Randomly generated invoice_id, allows to refer to this invoice.
    pub invoice_id: InvoiceId,
    /// Currency in use
    pub currency: Currency,
    /// Total amount of credits to be paid.
    pub total_dest_payment: u128,
}

/// Start an invoice (A request for payment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckClosePayment {
    pub payment_id: PaymentId,
    pub ack_uid: Uid,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunderControl<B> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    AddFriend(AddFriend<B>),
    RemoveFriend(RemoveFriend),
    SetFriendStatus(SetFriendStatus),
    SetFriendCurrencyMaxDebt(SetFriendCurrencyMaxDebt),
    SetFriendRelays(SetFriendRelays<B>),
    SetFriendName(SetFriendName),
    SetFriendCurrencyRate(SetFriendCurrencyRate),
    SetFriendCurrencyRequestsStatus(SetFriendCurrencyRequestsStatus),
    RemoveFriendCurrency(RemoveFriendCurrency),
    ResetFriendChannel(ResetFriendChannel),
    // Buyer API:
    CreatePayment(CreatePayment),
    CreateTransaction(CreateTransaction),
    RequestClosePayment(PaymentId),
    AckClosePayment(AckClosePayment),
    // Seller API:
    AddInvoice(AddInvoice),
    CancelInvoice(InvoiceId),
    // CommitInvoice(Commit),
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestResult {
    // Complete(Commit),
    Success,
    // TODO: Should we add more information to the failure here?
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionResult {
    pub request_id: Uid,
    pub result: RequestResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentStatusSuccess {
    // pub receipt: Receipt,
    pub ack_uid: Uid,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentStatus {
    PaymentNotFound,
    Success(PaymentStatusSuccess),
    Canceled(Uid), // ack_id
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseClosePayment {
    pub payment_id: PaymentId,
    pub status: PaymentStatus,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum FunderOutgoingControl {
    TransactionResult(TransactionResult),
    ResponseClosePayment(ResponseClosePayment),
    // ReportMutations(FunderReportMutations<B>),
}

impl Currency {
    pub fn as_str(&self) -> &str {
        &self.currency
    }
}

#[derive(Debug)]
pub enum CurrencyError {
    CurrencyNameTooLong,
}

impl TryFrom<String> for Currency {
    type Error = CurrencyError;
    fn try_from(currency: String) -> Result<Self, Self::Error> {
        if currency.len() > MAX_CURRENCY_LEN {
            return Err(CurrencyError::CurrencyNameTooLong);
        }
        Ok(Currency { currency })
    }
}

impl FromStr for Currency {
    type Err = CurrencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_CURRENCY_LEN {
            return Err(CurrencyError::CurrencyNameTooLong);
        }
        Ok(Currency {
            currency: s.to_owned(),
        })
    }
}

/*
impl BalanceInfo {
    fn flip(self) -> BalanceInfo {
        BalanceInfo {
            balance: self.balance.checked_neg().unwrap(),
            local_pending_debt: self.remote_pending_debt,
            remote_pending_debt: self.local_pending_debt,
        }
    }
}
*/

/*
impl McBalance {
    pub fn flip(self) -> McInfo {
        Self {
            balance: self.balance.checked_neg().unwrap(),
            local_pending_debt: self.remote_pending_debt,
            remote_pending_debt: self.local_pending_debt,
            in_fees: self.out_fees,
            out_fees: self.in_fees,
        }
    }
}
*/

/*
impl TokenInfo {
    pub fn flip(self) -> TokenInfo {
        let balances = self
            .balances
            .into_iter()
            .map(|currency_balance_info| CurrencyBalanceInfo {
                currency: currency_balance_info.currency,
                balance_info: currency_balance_info.balance_info.flip(),
            })
            .collect();
        TokenInfo {
            local_public_key: self.remote_public_key,
            remote_public_key: self.local_public_key,
            balances,
            move_token_counter: self.move_token_counter,
        }
    }
}
*/
