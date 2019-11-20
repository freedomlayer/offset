use serde::{Deserialize, Serialize};

use app::common::{
    Currency, InvoiceId, NamedIndexServerAddress, NamedRelayAddress, PublicKey, Rate, RelayAddress,
    Signature, Uid,
};
use app::ser_string::{from_base64, from_string, to_base64, to_string};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    inner: Vec<u8>,
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
    Fees(u128),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsePayInvoice {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    pub response: ResponsePayInvoiceInner,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayInvoiceResultInner {
    Failure,
    Success(Commit),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayInvoiceDone {
    Failure,
    Success,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayInvoiceResult {
    pub invoice_id: InvoiceId,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserResponse {
    /// Funds:
    ResponsePayInvoice(ResponsePayInvoice),
    PayInvoiceResult(PayInvoiceResult),
    PayInvoiceDone(PayInvoiceDone),
    // /// Reports about current state:
    // Report(NodeReport),
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum UserRequest {
    /// Manage locally used relays:
    AddRelay(NamedRelayAddress),
    RemoveRelay(PublicKey),
    /// Manage index servers:
    AddIndexServer(NamedIndexServerAddress),
    RemoveIndexServer(PublicKey),
    /// Friend management:
    AddFriend(AddFriend),
    SetFriendRelays(SetFriendRelays),
    SetFriendName(SetFriendName),
    RemoveFriend(PublicKey),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriendCurrency(OpenFriendCurrency),
    CloseFriendCurrency(CloseFriendCurrency),
    SetFriendCurrencyMaxDebt(SetFriendCurrencyMaxDebt),
    SetFriendCurrencyRate(SetFriendCurrencyRate),
    RemoveFriendCurrency(RemoveFriendCurrency),
    ResetFriendChannel(ResetFriendChannel),
    /// Buyer:
    RequestPayInvoice(RequestPayInvoice),
    // ConfirmPayInvoice(ConfirmPayInvoice),
    CancelPayInvoice(InvoiceId),
    /// Seller:
    AddInvoice(AddInvoice),
    CancelInvoice(InvoiceId),
    CommitInvoice(Commit),
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
