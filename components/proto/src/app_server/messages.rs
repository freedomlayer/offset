use common::canonical_serialize::CanonicalSerialize;
use crypto::identity::PublicKey;

use crate::funder::messages::{UserRequestSendFunds, ResponseReceived,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::funder::scheme::FunderScheme;
use crate::report::messages::{FunderReport, FunderReportMutation};
use crate::index_client::messages::{IndexClientReport, 
    IndexClientReportMutation, ClientResponseRoutes};
use crate::index_server::messages::RequestRoutes;
use crate::net::messages::NetAddress;

use index_client::messages::AddIndexServer;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NamedRelayAddress<B=NetAddress> {
    pub public_key: PublicKey,
    pub address: B,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayAddress<B=NetAddress> {
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
pub struct NodeReport<FS:FunderScheme,ISA> {
    pub funder_report: FunderReport<FS>,
    pub index_client_report: IndexClientReport<ISA>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeReportMutation<FS:FunderScheme,ISA> {
    Funder(FunderReportMutation<FS>),
    IndexClient(IndexClientReportMutation<ISA>),
}


#[derive(Debug, PartialEq, Eq)]
pub enum AppServerToApp<FS: FunderScheme,ISA> {
    /// Funds:
    ResponseReceived(ResponseReceived),
    /// Reports about current state:
    Report(NodeReport<FS,ISA>),
    ReportMutations(Vec<NodeReportMutation<FS,ISA>>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug, PartialEq, Eq)]
pub enum AppToAppServer<RA=Vec<RelayAddress>,NRA=Vec<NamedRelayAddress>,ISA=NetAddress> {
    /// Set relay address to be used locally:
    SetRelays(NRA),
    /// Sending funds:
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<RA>),
    SetFriendRelays(SetFriendAddress<RA>),
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

impl<FS,ISA> NodeReport<FS,ISA> 
where
    FS: FunderScheme,
    ISA: Eq + Clone,
{
    pub fn mutate(&mut self, mutation: &NodeReportMutation<FS,ISA>) 
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
