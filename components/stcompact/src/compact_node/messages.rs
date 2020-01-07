use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use app::common::{
    Currency, HashResult, HashedLock, InvoiceId, NamedIndexServerAddress, NamedRelayAddress,
    PaymentId, PlainLock, PublicKey, RandValue, Rate, Receipt, RelayAddress, Signature, Uid,
};
use common::ser_utils::{
    ser_b64, ser_map_b64_any, ser_map_str_any, ser_map_str_str, ser_option_b64, ser_string,
};

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct OpenFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CloseFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddFriend {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
    pub name: String,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendRelays {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendName {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub name: String,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct InitPayment {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_b64")]
    pub dest_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub dest_payment: u128,
    /// Short textual invoice description
    pub description: String,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentFeesResponse {
    Unreachable,
    Fees(
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (fees, confirm_id)
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentFees {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    pub response: PaymentFeesResponse,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum PaymentDone {
    #[serde(with = "ser_b64")]
    Failure(Uid), // ack_uid
    Success(
        Receipt,
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (receipt, fees, ack_uid)
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum ResponseCommitInvoice {
    Failure,
    Success,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct PaymentCommit {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    pub commit: Commit,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct ConfirmPaymentFees {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    #[serde(with = "ser_b64")]
    pub confirm_id: Uid,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendCurrencyMaxDebt {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub remote_max_debt: u128,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetFriendChannel {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_b64")]
    pub reset_token: Signature,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFriendCurrencyRate {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    pub rate: Rate,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddInvoice {
    /// Randomly generated invoice_id, allows to refer to this invoice.
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    /// Currency in use
    #[serde(with = "ser_string")]
    pub currency: Currency,
    /// Total amount of credits to be paid.
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    /// Short textual description for the invoice
    pub description: String,
}

#[derive(Arbitrary, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct ConfigReport {
    /// Rate of forwarding transactions that arrived from this friend to any other friend
    /// for a certain currency.
    pub rate: Rate,
    /// Credit frame for the remote side (Set by the user of this node)
    #[serde(with = "ser_string")]
    pub remote_max_debt: u128,
    /// Can requests be sent through this node (Incoming or outgoing)?
    /// If `false`, only the local user may send or receive requests through this node.
    pub is_open: bool,
}

#[derive(Arbitrary, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetTermsReport {
    #[serde(with = "ser_b64")]
    pub reset_token: Signature,
    #[serde(with = "ser_map_str_str")]
    pub balance_for_reset: HashMap<Currency, i128>,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInconsistentReport {
    #[serde(with = "ser_map_str_str")]
    pub local_reset_terms: HashMap<Currency, i128>,
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McBalanceReport {
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
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyReport {
    pub balance: McBalanceReport,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelConsistentReport {
    #[serde(with = "ser_map_str_any")]
    pub currency_reports: HashMap<Currency, CurrencyReport>,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(ChannelConsistentReport),
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct BalanceInfo {
    #[serde(with = "ser_string")]
    pub balance: i128,
    #[serde(with = "ser_string")]
    pub local_pending_debt: u128,
    #[serde(with = "ser_string")]
    pub remote_pending_debt: u128,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct McInfo {
    #[serde(with = "ser_b64")]
    pub local_public_key: PublicKey,
    #[serde(with = "ser_b64")]
    pub remote_public_key: PublicKey,
    #[serde(with = "ser_map_str_any")]
    pub balances: HashMap<Currency, BalanceInfo>,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct CountersInfo {
    pub inconsistency_counter: u64,
    #[serde(with = "ser_string")]
    pub move_token_counter: u128,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub mc: McInfo,
    pub counters: CountersInfo,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveTokenHashedReport {
    #[serde(with = "ser_b64")]
    pub prefix_hash: HashResult,
    pub token_info: TokenInfo,
    #[serde(with = "ser_b64")]
    pub rand_nonce: RandValue,
    #[serde(with = "ser_b64")]
    pub new_token: Signature,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendReport {
    pub name: String,
    #[serde(with = "ser_map_str_any")]
    pub currency_configs: HashMap<Currency, ConfigReport>,
    /// Last message signed by the remote side.
    /// Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport>,
    // TODO: The state of liveness = true with status = disabled should never happen.
    // Can we somehow express this in the type system?
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport,
    pub status: FriendStatusReport,
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenInvoice {
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    /// Invoice description
    pub description: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpenPaymentStatus {
    SearchingRoute(#[serde(with = "ser_b64")] Uid), // request_routes_id
    FoundRoute(
        #[serde(with = "ser_b64")] Uid,
        #[serde(with = "ser_string")] u128,
    ), // (confirm_id, fees)
    Sending(#[serde(with = "ser_string")] u128),    // fees
    Commit(Commit, #[serde(with = "ser_string")] u128), // (commit, fees)
    Success(
        Receipt,
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (Receipt, fees, ack_uid)
    Failure(#[serde(with = "ser_b64")] Uid),        // ack_uid
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenPayment {
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_b64")]
    pub dest_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub dest_payment: u128,
    /// Invoice description (Obtained from the corresponding invoice)
    pub description: String,
    /// Current status of open payment
    pub status: OpenPaymentStatus,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactReport {
    #[serde(with = "ser_b64")]
    pub local_public_key: PublicKey,
    pub index_servers: Vec<NamedIndexServerAddress>,
    #[serde(with = "ser_option_b64")]
    pub opt_connected_index_server: Option<PublicKey>,
    pub relays: Vec<NamedRelayAddress>,
    #[serde(with = "ser_map_b64_any")]
    pub friends: HashMap<PublicKey, FriendReport>,
    /// Seller's open invoices:
    #[serde(with = "ser_map_b64_any")]
    pub open_invoices: HashMap<InvoiceId, OpenInvoice>,
    /// Buyer's open payments:
    #[serde(with = "ser_map_b64_any")]
    pub open_payments: HashMap<PaymentId, OpenPayment>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactToUserAck {
    /// Acknowledge the receipt of `UserToCompact`
    /// Should be sent after `Report`, in case any changes occured.
    Ack(#[serde(with = "ser_b64")] Uid),
    CompactToUser(CompactToUser),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactToUser {
    // ------------[Buyer]------------------
    /// Response: Shows required fees, or states that the destination is unreachable:
    PaymentFees(PaymentFees),
    /// Result: Possibly returns the Commit (Should be delivered out of band)
    PaymentCommit(PaymentCommit),
    /// Done: Possibly returns a Receipt or failure
    PaymentDone(PaymentDone),
    // ------------[Seller]-------------------
    ResponseCommitInvoice(ResponseCommitInvoice),
    // ------------[Reports]-------------------
    /// Reports about current state:
    Report(CompactReport),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum UserToCompact {
    // ----------------[Configuration]-----------------------
    /// Manage locally used relays:
    AddRelay(NamedRelayAddress),
    #[serde(with = "ser_b64")]
    RemoveRelay(PublicKey),
    /// Manage index servers:
    AddIndexServer(NamedIndexServerAddress),
    #[serde(with = "ser_b64")]
    RemoveIndexServer(PublicKey),
    /// Friend management:
    AddFriend(AddFriend),
    SetFriendRelays(SetFriendRelays),
    SetFriendName(SetFriendName),
    #[serde(with = "ser_b64")]
    RemoveFriend(PublicKey),
    #[serde(with = "ser_b64")]
    EnableFriend(PublicKey),
    #[serde(with = "ser_b64")]
    DisableFriend(PublicKey),
    OpenFriendCurrency(OpenFriendCurrency),
    CloseFriendCurrency(CloseFriendCurrency),
    SetFriendCurrencyMaxDebt(SetFriendCurrencyMaxDebt),
    SetFriendCurrencyRate(SetFriendCurrencyRate),
    RemoveFriendCurrency(RemoveFriendCurrency),
    ResetFriendChannel(ResetFriendChannel),
    // ---------------[Buyer]------------------------------
    // Request sending an amount to some desination:
    InitPayment(InitPayment),
    // Confirm sending fees:
    ConfirmPaymentFees(ConfirmPaymentFees),
    #[serde(with = "ser_b64")]
    CancelPayment(PaymentId),
    AckPaymentDone(PaymentId, Uid), // (payment_id, ack_uid)
    // ---------------[Seller]------------------------------
    AddInvoice(AddInvoice),
    #[serde(with = "ser_b64")]
    CancelInvoice(InvoiceId),
    RequestCommitInvoice(Commit),
    // ---------------[Verification]------------------------
    // TODO: Add API for verification of receipt and last token?
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct UserToCompactAck {
    #[serde(with = "ser_b64")]
    pub user_request_id: Uid,
    pub inner: UserToCompact,
}

/*
#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
