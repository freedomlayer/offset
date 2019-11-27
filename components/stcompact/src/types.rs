use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use app::common::{
    Currency, HashResult, HashedLock, InvoiceId, NamedIndexServerAddress, NamedRelayAddress,
    PlainLock, PublicKey, RandValue, Rate, RelayAddress, Signature, Uid,
};
use app::ser_string::{from_base64, from_string, to_base64, to_string};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub response_hash: HashResult,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub src_plain_lock: PlainLock,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_hashed_lock: HashedLock,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub signature: Signature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct OpenFriendCurrency {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CloseFriendCurrency {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFriend {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendRelays {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendName {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    pub name: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct RequestPayInvoice {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponsePayInvoiceInner {
    Unreachable,
    Fees(u128, Uid), // (fees, confirm_id)
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePayInvoice {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    pub response: ResponsePayInvoiceInner,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum PayInvoiceDone {
    Failure,
    Success,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum PayInvoiceResultInner {
    Failure,
    Success(Commit),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct PayInvoiceResult {
    pub invoice_id: InvoiceId,
    pub result: PayInvoiceResultInner,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct ConfirmPayInvoice {
    pub invoice_id: InvoiceId,
    pub confirm_id: Uid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendCurrencyMaxDebt {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub remote_max_debt: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveFriendCurrency {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetFriendChannel {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub reset_token: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendCurrencyRate {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub friend_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    pub rate: Rate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddInvoice {
    /// Randomly generated invoice_id, allows to refer to this invoice.
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    /// Currency in use
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    /// Total amount of credits to be paid.
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct CurrencyConfigReport {
    /// Rate of forwarding transactions that arrived from this friend to any other friend
    /// for a certain currency.
    pub rate: Rate,
    /// Wanted credit frame for the remote side (Set by the user of this node)
    /// It might take a while until this value is applied, as it needs to be communicated to the
    /// remote side.
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub wanted_remote_max_debt: u128,
    /// Can the remote friend send requests through us? This is a value chosen by the user, and it
    /// might take some time until it is applied (As it should be communicated to the remote
    /// friend).
    pub wanted_local_requests_status: RequestsStatusReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetTermsReport {
    pub reset_token: Signature,
    pub balance_for_reset: HashMap<Currency, i128>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInconsistentReport {
    pub local_reset_terms: HashMap<Currency, u128>,
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalanceReport {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub balance: i128,
    /// Maximum possible local debt
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub local_max_debt: u128,
    /// Maximum possible remote debt
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub remote_max_debt: u128,
    /// Frozen credits by our side
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub remote_pending_debt: u128,
}

#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct McRequestsStatusReport {
    /// Local is open/closed for incoming requests:
    pub local: RequestsStatusReport,
    /// Remote is open/closed for incoming requests:
    pub remote: RequestsStatusReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyReport {
    pub balance: McBalanceReport,
    pub requests_status: McRequestsStatusReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelConsistentReport {
    pub currency_reports: HashMap<Currency, CurrencyReport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(ChannelConsistentReport),
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct BalanceInfo {
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub balance: i128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub local_pending_debt: u128,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub remote_pending_debt: u128,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CurrencyBalanceInfo {
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    pub balance_info: BalanceInfo,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct McInfo {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub local_public_key: PublicKey,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub remote_public_key: PublicKey,
    pub balances: Vec<CurrencyBalanceInfo>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CountersInfo {
    pub inconsistency_counter: u64,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub move_token_counter: u128,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub mc: McInfo,
    pub counters: CountersInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveTokenHashedReport {
    pub prefix_hash: HashResult,
    pub token_info: TokenInfo,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendReport {
    pub name: String,
    pub currency_configs: HashMap<Currency, CurrencyConfigReport>,
    /// Last message signed by the remote side.
    /// Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    // TODO: The state of liveness = true with status = disabled should never happen.
    // Can we somehow express this in the type system?
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub status: FriendStatusReport,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeReport {
    pub local_public_key: PublicKey,
    pub index_servers: Vec<NamedIndexServerAddress>,
    pub opt_connected_index_server: Option<NamedIndexServerAddress>,
    pub relays: Vec<NamedRelayAddress>,
    pub friends: HashMap<PublicKey, FriendReport>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToUser {
    // ------------[Payments]------------------
    /// Response: Shows required fees, or states that the destination is unreachable:
    ResponsePayInvoice(ResponsePayInvoice),
    /// Result: Possibly returns the Commit (Should be delivered out of band)
    PayInvoiceResult(PayInvoiceResult),
    /// Done: Possibly returns a Receipt or failure
    PayInvoiceDone(PayInvoiceDone),
    // ------------[Reports]-------------------
    /// Acknowledge the receipt of `UserRequest`
    /// Should be sent after `Report`, in case any changes occured.
    Ack(Uid),
    /// Reports about current state:
    Report(NodeReport),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum UserRequest {
    // ----------------[Configuration]-----------------------
    /// Manage locally used relays:
    AddRelay(NamedRelayAddress),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    RemoveRelay(PublicKey),
    /// Manage index servers:
    AddIndexServer(NamedIndexServerAddress),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    RemoveIndexServer(PublicKey),
    /// Friend management:
    AddFriend(AddFriend),
    SetFriendRelays(SetFriendRelays),
    SetFriendName(SetFriendName),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    RemoveFriend(PublicKey),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    EnableFriend(PublicKey),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    DisableFriend(PublicKey),
    OpenFriendCurrency(OpenFriendCurrency),
    CloseFriendCurrency(CloseFriendCurrency),
    SetFriendCurrencyMaxDebt(SetFriendCurrencyMaxDebt),
    SetFriendCurrencyRate(SetFriendCurrencyRate),
    RemoveFriendCurrency(RemoveFriendCurrency),
    ResetFriendChannel(ResetFriendChannel),
    // ---------------[Buyer]------------------------------
    // Request sending an amount to some desination:
    RequestPayInvoice(RequestPayInvoice),
    // Confirm sending fees:
    ConfirmPayInvoice(ConfirmPayInvoice),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    CancelPayInvoice(InvoiceId),
    // ---------------[Seller]------------------------------
    AddInvoice(AddInvoice),
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    CancelInvoice(InvoiceId),
    CommitInvoice(Commit),
    // ---------------[Verification]------------------------
    // TODO: Add API for verification of receipt and last token?
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct FromUser {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub user_request_id: Uid,
    pub user_request: UserRequest,
}

/*
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPermissions {
    /// Can request routes
    pub routes: bool,
    /// Can send credits as a buyer
    pub buyer: bool,
    /// Can receive credits as a seller
    pub seller: bool,
    /// Can configure friends
    pub config: bool,
}
*/
