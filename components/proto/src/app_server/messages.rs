use serde::{Deserialize, Serialize};

use common::ser_utils::ser_b64;

use crate::crypto::{InvoiceId, NodePort, PaymentId, PublicKey, Uid};
// use crate::wrapper::Wrapper;

use crate::funder::messages::{
    AckClosePayment, AddFriend, AddInvoice, CreatePayment, CreateTransaction, Currency,
    RemoveFriendCurrency, ResetFriendChannel, ResponseClosePayment, SetFriendCurrencyMaxDebt,
    SetFriendCurrencyRate, SetFriendName, SetFriendRelays, TransactionResult,
};
use crate::index_client::messages::ClientResponseRoutes;
use crate::index_server::messages::{NamedIndexServerAddress, RequestRoutes};
use crate::net::messages::NetAddress;
// use crate::report::messages::{FunderReport, FunderReportMutation};

// TODO: Move NamedRelayAddress and RelayAddress to another place in offset-proto?

#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedRelayAddress<B = NetAddress> {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub address: B,
    pub name: String,
}

// TODO: Remove port from RelayAddress? Create another struct?
#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayAddress<B = NetAddress> {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub address: B,
}

// TODO: Remove port from RelayAddress? Create another struct?
#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayAddressPort {
    #[serde(with = "ser_b64")]
    pub public_key: PublicKey,
    pub address: NetAddress,
    #[serde(with = "ser_b64")]
    pub port: NodePort,
}

/*
impl<B> From<NamedRelayAddress<B>> for RelayAddress<B> {
    fn from(from: NamedRelayAddress<B>) -> Self {
        RelayAddress {
            public_key: from.public_key,
            address: from.address,
        }
    }
}
*/

/*
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReport<B = NetAddress> {
    pub funder_report: FunderReport<B>,
    pub index_client_report: IndexClientReport<B>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeReportMutation<B = NetAddress> {
    Funder(FunderReportMutation<B>),
    IndexClient(IndexClientReportMutation<B>),
}
*/

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptAppRequestId {
    AppRequestId(Uid),
    Empty,
}

impl From<Option<Uid>> for OptAppRequestId {
    fn from(opt: Option<Uid>) -> Self {
        match opt {
            Some(uid) => OptAppRequestId::AppRequestId(uid),
            None => OptAppRequestId::Empty,
        }
    }
}

impl From<OptAppRequestId> for Option<Uid> {
    fn from(opt: OptAppRequestId) -> Self {
        match opt {
            OptAppRequestId::AppRequestId(uid) => Some(uid),
            OptAppRequestId::Empty => None,
        }
    }
}

/*
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportMutations<B = NetAddress> {
    pub opt_app_request_id: Option<Uid>,
    pub mutations: Vec<NodeReportMutation<B>>,
}
*/

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq)]
pub enum AppServerToApp {
    /// Funds:
    TransactionResult(TransactionResult),
    ResponseClosePayment(ResponseClosePayment),
    // /// Reports about current state:
    // Report(NodeReport<B>),
    // ReportMutations(ReportMutations<B>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug, PartialEq, Eq)]
pub enum NamedRelaysMutation<B = NetAddress> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OpenFriendCurrency {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CloseFriendCurrency {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AppRequest<B = NetAddress> {
    /// Manage locally used relays:
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
    /// Friend management:
    AddFriend(AddFriend<B>),
    SetFriendRelays(SetFriendRelays<B>),
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
    CreatePayment(CreatePayment),
    CreateTransaction(CreateTransaction),
    RequestClosePayment(PaymentId),
    AckClosePayment(AckClosePayment),
    /// Seller:
    AddInvoice(AddInvoice),
    CancelInvoice(InvoiceId),
    //CommitInvoice(Commit),
    /// Request routes from one node to another:
    RequestRoutes(RequestRoutes),
    /// Manage index servers:
    AddIndexServer(NamedIndexServerAddress<B>),
    RemoveIndexServer(PublicKey),
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AppToAppServer<B = NetAddress> {
    pub app_request_id: Uid,
    pub app_request: AppRequest<B>,
}

impl<B> AppToAppServer<B> {
    pub fn new(app_request_id: Uid, app_request: AppRequest<B>) -> Self {
        AppToAppServer {
            app_request_id,
            app_request,
        }
    }
}

// TODO: Move this code to a separate module:

/*
#[derive(Debug)]
pub struct NodeReportMutateError;

impl<B> NodeReport<B>
where
    B: Eq + Clone,
{
    pub fn mutate(
        &mut self,
        mutation: &NodeReportMutation<B>,
    ) -> Result<(), NodeReportMutateError> {
        match mutation {
            NodeReportMutation::<B>::Funder(mutation) => self
                .funder_report
                .mutate(mutation)
                .map_err(|_| NodeReportMutateError)?,
            NodeReportMutation::<B>::IndexClient(mutation) => {
                self.index_client_report.mutate(mutation)
            }
        };
        Ok(())
    }
}

impl<B> MutableState for NodeReport<B>
where
    B: Eq + Clone,
{
    type Mutation = NodeReportMutation<B>;
    type MutateError = NodeReportMutateError;

    fn mutate(&mut self, mutation: &NodeReportMutation<B>) -> Result<(), NodeReportMutateError> {
        self.mutate(mutation)
    }
}
*/

#[derive(Arbitrary, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
