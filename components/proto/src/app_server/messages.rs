use crypto::identity::PublicKey;

use crate::funder::messages::{UserRequestSendFunds, ResponseReceived,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::funder::report::{FunderReport, FunderReportMutation};
use crate::index_client::messages::{IndexClientReport, IndexClientReportMutation};
use crate::index_server::messages::{RequestRoutes, ResponseRoutes};


#[derive(Debug)]
pub struct NodeReport<B:Clone,ISA> {
    funder_report: FunderReport<Vec<B>>,
    index_client_report: IndexClientReport<ISA>,
}

#[derive(Debug)]
pub enum NodeReportMutation<B,ISA> {
    Funder(FunderReportMutation<Vec<B>>),
    IndexClient(IndexClientReportMutation<ISA>),
}

#[allow(unused)]
#[derive(Debug)]
pub enum AppServerToApp<B: Clone,ISA> {
    /// Funds:
    ResponseReceived(ResponseReceived),
    /// Reports about current state:
    Report(NodeReport<B,ISA>),
    ReportMutations(Vec<NodeReportMutation<B,ISA>>),
    ResponseRoute(ResponseRoutes),
}

#[allow(unused)]
#[derive(Debug)]
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
