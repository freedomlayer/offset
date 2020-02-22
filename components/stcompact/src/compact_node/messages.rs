use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use app::common::{
    Currency, HashResult, HashedLock, InvoiceId, NamedIndexServerAddress, NamedRelayAddress,
    PaymentId, PlainLock, PublicKey, RandValue, Rate, Receipt, RelayAddress, Signature, Uid,
};
use common::ser_utils::{
    ser_b64, ser_map_b64_any, ser_map_str_any, ser_map_str_str, ser_option_b64, ser_string,
};

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Generation(#[serde(with = "ser_string")] pub u64);

impl Generation {
    pub fn new() -> Self {
        Generation(0)
    }

    /// Advance generation, and return the current (old) generation value
    pub fn advance(&mut self) -> Self {
        let current = self.clone();
        // We crash if we ever issue 2**64 transactions.
        self.0 = self.0.checked_add(1).unwrap();
        current
    }
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct OpenFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFriend {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
    pub name: String,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFriendRelays {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub relays: Vec<RelayAddress>,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFriendName {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    pub name: String,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub enum PaymentFeesResponse {
    Unreachable,
    Fees(
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (fees, confirm_id)
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentFees {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    pub response: PaymentFeesResponse,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PaymentDoneStatus {
    #[serde(with = "ser_b64")]
    Failure(Uid), // ack_uid
    Success(
        Receipt,
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (receipt, fees, ack_uid)
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDone {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    pub status: PaymentDoneStatus,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestVerifyCommit {
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    // #[serde(with = "ser_b64")]
    // pub seller_public_key: PublicKey,
    pub commit: Commit,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResponseVerifyCommit {
    #[serde(with = "ser_b64")]
    pub request_id: Uid,
    pub status: VerifyCommitStatus,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum VerifyCommitStatus {
    Failure,
    Success,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaymentCommit {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    pub commit: Commit,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPaymentFees {
    #[serde(with = "ser_b64")]
    pub payment_id: PaymentId,
    #[serde(with = "ser_b64")]
    pub confirm_id: Uid,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFriendCurrencyMaxDebt {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub remote_max_debt: u128,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveFriendCurrency {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetFriendChannel {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_b64")]
    pub reset_token: Signature,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFriendCurrencyRate {
    #[serde(with = "ser_b64")]
    pub friend_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    pub rate: Rate,
}

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

// TODO; Who uses this enum?
#[derive(Arbitrary, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum RequestsStatusReport {
    Open,
    Closed,
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetTermsReport {
    #[serde(with = "ser_b64")]
    pub reset_token: Signature,
    #[serde(with = "ser_map_str_str")]
    pub balance_for_reset: HashMap<Currency, i128>,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInconsistentReport {
    #[serde(with = "ser_map_str_str")]
    pub local_reset_terms: HashMap<Currency, i128>,
    pub opt_remote_reset_terms: Option<ResetTermsReport>,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyReport {
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
#[serde(rename_all = "camelCase")]
pub struct ChannelConsistentReport {
    #[serde(with = "ser_map_str_any")]
    pub currency_reports: HashMap<Currency, CurrencyReport>,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistentReport),
    Consistent(ChannelConsistentReport),
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FriendStatusReport {
    Enabled,
    Disabled,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceInfo {
    #[serde(with = "ser_string")]
    pub balance: i128,
    #[serde(with = "ser_string")]
    pub local_pending_debt: u128,
    #[serde(with = "ser_string")]
    pub remote_pending_debt: u128,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McInfo {
    #[serde(with = "ser_b64")]
    pub local_public_key: PublicKey,
    #[serde(with = "ser_b64")]
    pub remote_public_key: PublicKey,
    #[serde(with = "ser_map_str_any")]
    pub balances: HashMap<Currency, BalanceInfo>,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountersInfo {
    #[serde(with = "ser_string")]
    pub inconsistency_counter: u64,
    #[serde(with = "ser_string")]
    pub move_token_counter: u128,
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub mc: McInfo,
    pub counters: CountersInfo,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct OpenInvoice {
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    /// Invoice description
    pub description: String,
    /// Do we already have a commitment for this invoice?
    pub is_commited: bool,
    /// Chronological counter
    pub generation: Generation,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
    /// Chronological counter
    pub generation: Generation,
    /// Current status of open payment
    pub status: OpenPaymentStatus,
}

#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub enum CompactToUserAck {
    /// Acknowledge the receipt of `UserToCompact`
    /// Should be sent after `Report`, in case any changes occured.
    Ack(#[serde(with = "ser_b64")] Uid),
    CompactToUser(CompactToUser),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactToUser {
    // TODO: Maybe in the future we will not need most of the message here,
    // and can keep only `Report` and `ResponseVerifyCommit`?
    // ------------[Buyer]------------------
    /// Response: Shows required fees, or states that the destination is unreachable:
    PaymentFees(PaymentFees),
    /// Result: Possibly returns the Commit (Should be delivered out of band)
    PaymentCommit(PaymentCommit),
    /// Done: Possibly returns a Receipt or failure
    PaymentDone(PaymentDone),
    // ------------[Reports]-------------------
    /// Reports about current state:
    Report(CompactReport),
    // -------------[Verify]-------------------
    ResponseVerifyCommit(ResponseVerifyCommit),
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    AckPaymentDone(
        #[serde(with = "ser_b64")] PaymentId,
        #[serde(with = "ser_b64")] Uid,
    ), // (payment_id, ack_uid)
    // ---------------[Seller]------------------------------
    AddInvoice(AddInvoice),
    #[serde(with = "ser_b64")]
    CancelInvoice(InvoiceId),
    RequestVerifyCommit(RequestVerifyCommit),
    #[serde(with = "ser_b64")]
    CommitInvoice(InvoiceId),
    // ---------------[Verification]------------------------
    // TODO: Add API for verification of receipt and last token?
}

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToCompactAck {
    // TODO: Possibly rename to `request_id`?
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
