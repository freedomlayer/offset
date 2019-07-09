use serde::{Deserialize, Serialize};

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use common::canonical_serialize::CanonicalSerialize;
use common::mutable_state::MutableState;

use crate::crypto::{InvoiceId, PaymentId, PublicKey, Uid};

use crate::funder::messages::{
    AckClosePayment, AddFriend, AddInvoice, CreatePayment, CreateTransaction, MultiCommit,
    ResetFriendChannel, ResponseClosePayment, SetFriendName, SetFriendRate, SetFriendRelays,
    SetFriendRemoteMaxDebt, TransactionResult,
};
use crate::index_client::messages::{
    ClientResponseRoutes, IndexClientReport, IndexClientReportMutation,
};
use crate::index_server::messages::{NamedIndexServerAddress, RequestRoutes};
use crate::net::messages::NetAddress;
use crate::report::messages::{FunderReport, FunderReportMutation};

// TODO: Move NamedRelayAddress and RelayAddress to another place in offst-proto?

#[capnp_conv(crate::common_capnp::named_relay_address)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedRelayAddress {
    pub public_key: PublicKey,
    pub address: NetAddress,
    pub name: String,
}

/* Generic:
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedRelayAddress<B = NetAddress> {
    pub public_key: PublicKey,
    pub address: B,
    pub name: String,
}
*/

#[capnp_conv(crate::common_capnp::relay_address)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayAddress {
    pub public_key: PublicKey,
    pub address: NetAddress,
}

/* Generic:
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayAddress<B = NetAddress> {
    pub public_key: PublicKey,
    pub address: B,
}
*/

impl From<NamedRelayAddress> for RelayAddress {
    fn from(from: NamedRelayAddress) -> Self {
        RelayAddress {
            public_key: from.public_key,
            address: from.address,
        }
    }
}

impl CanonicalSerialize for RelayAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes.extend_from_slice(&self.address.canonical_serialize());
        res_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReport<B = NetAddress>
where
    B: Clone,
{
    pub funder_report: FunderReport,
    pub index_client_report: IndexClientReport<B>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeReportMutation<B = NetAddress>
where
    B: Clone,
{
    Funder(FunderReportMutation<B>),
    IndexClient(IndexClientReportMutation<B>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportMutations<B = NetAddress>
where
    B: Clone,
{
    pub opt_app_request_id: Option<Uid>,
    pub mutations: Vec<NodeReportMutation<B>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq)]
pub enum AppServerToApp<B = NetAddress>
where
    B: Clone,
{
    /// Funds:
    TransactionResult(TransactionResult),
    ResponseClosePayment(ResponseClosePayment),
    /// Reports about current state:
    Report(NodeReport<B>),
    ReportMutations(ReportMutations<B>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug, PartialEq, Eq)]
pub enum NamedRelaysMutation<B = NetAddress> {
    AddRelay(NamedRelayAddress<B>),
    RemoveRelay(PublicKey),
}

#[derive(Debug, PartialEq, Eq)]
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
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    SetFriendRate(SetFriendRate),
    ResetFriendChannel(ResetFriendChannel),
    /// Buyer:
    CreatePayment(CreatePayment),
    CreateTransaction(CreateTransaction),
    RequestClosePayment(PaymentId),
    AckClosePayment(AckClosePayment),
    /// Seller:
    AddInvoice(AddInvoice),
    CancelInvoice(InvoiceId),
    CommitInvoice(MultiCommit),
    /// Request routes from one node to another:
    RequestRoutes(RequestRoutes),
    /// Manage index servers:
    AddIndexServer(NamedIndexServerAddress<B>),
    RemoveIndexServer(PublicKey),
}
#[derive(Debug, PartialEq, Eq)]
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
            NodeReportMutation::Funder(mutation) => self
                .funder_report
                .mutate(mutation)
                .map_err(|_| NodeReportMutateError)?,
            NodeReportMutation::IndexClient(mutation) => self.index_client_report.mutate(mutation),
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
