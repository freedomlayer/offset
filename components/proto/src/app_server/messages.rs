use crypto::identity::PublicKey;

use crate::funder::messages::{UserRequestSendFunds, ResponseReceived,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::report::messages::{FunderReport, FunderReportMutation};
use crate::index_client::messages::{IndexClientReport, 
    IndexClientReportMutation, ClientResponseRoutes};
use crate::index_server::messages::RequestRoutes;


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReport<B:Clone,ISA> {
    pub funder_report: FunderReport<Vec<B>>,
    pub index_client_report: IndexClientReport<ISA>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeReportMutation<B,ISA> {
    Funder(FunderReportMutation<Vec<B>>),
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
    SetRelays(Vec<B>), 
    /// Sending funds:
    RequestSendFunds(UserRequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<Vec<B>>),
    SetFriendRelays(SetFriendAddress<Vec<B>>),
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
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
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
