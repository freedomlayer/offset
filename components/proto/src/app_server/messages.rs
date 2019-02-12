use common::canonical_serialize::CanonicalSerialize;
use crypto::identity::PublicKey;

use crate::funder::messages::{UserRequestSendFunds, ResponseReceived,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::report::messages::{FunderReport, FunderReportMutation};
use crate::index_client::messages::{IndexClientReport, 
    IndexClientReportMutation, ClientResponseRoutes};
use crate::index_server::messages::RequestRoutes;

use index_client::messages::AddIndexServer;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedRelayAddress<B> {
    pub public_key: PublicKey,
    pub address: B,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayAddress<B> {
    pub public_key: PublicKey,
    pub address: B,
}

impl<B> CanonicalSerialize for RelayAddress<B> 
where
    B: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.public_key);
        res_bytes.extend_from_slice(&self.address.canonical_serialize());
        res_bytes
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReport<B:Clone,ISA> {
    pub funder_report: FunderReport<Vec<RelayAddress<B>>>,
    pub index_client_report: IndexClientReport<ISA>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeReportMutation<B,ISA> {
    Funder(FunderReportMutation<Vec<RelayAddress<B>>>),
    IndexClient(IndexClientReportMutation<ISA>),
}


#[derive(Debug, PartialEq, Eq)]
pub enum AppServerToApp<B: Clone,ISA> {
    /// Funds:
    ResponseReceived(ResponseReceived),
    /// Reports about current state:
    Report(NodeReport<B,ISA>),
    ReportMutations(Vec<NodeReportMutation<B,ISA>>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug, PartialEq, Eq)]
pub enum AppToAppServer<B,ISA> {
    /// Set relay address to be used locally:
    SetRelays(Vec<NamedRelayAddress<B>>), 
    /// Sending funds:
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<Vec<RelayAddress<B>>>),
    SetFriendRelays(SetFriendAddress<Vec<RelayAddress<B>>>),
    SetFriendName(SetFriendName),
    RemoveFriend(PublicKey),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
    /// Request routes from one node to another:
    RequestRoutes(RequestRoutes),
    /// Manage index servers:
    AddIndexServer(AddIndexServer<ISA>),
    RemoveIndexServer(PublicKey),
}


#[derive(Debug)]
pub struct NodeReportMutateError;

impl<B,ISA> NodeReport<B,ISA> 
where
    B: Clone,
    ISA: Eq + Clone,
{
    pub fn mutate(&mut self, mutation: &NodeReportMutation<B,ISA>) 
        -> Result<(), NodeReportMutateError> {

        match mutation {
            NodeReportMutation::Funder(mutation) => 
                self.funder_report.mutate(mutation)
                    .map_err(|_| NodeReportMutateError)?,
            NodeReportMutation::IndexClient(mutation) => 
                self.index_client_report.mutate(mutation),
        };
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPermissions {
    /// Receives reports about state
    pub reports: bool,
    /// Can request routes
    pub routes: bool,
    /// Can send credits
    pub send_funds: bool,
    /// Can configure friends
    pub config: bool,
}
