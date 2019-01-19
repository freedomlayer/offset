use crypto::identity::PublicKey;
use crypto::crypto_rand::RandValue;

use crate::funder::messages::{RequestSendFunds, ResponseSendFunds,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, RemoveFriend,
                            SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::funder::report::{FunderReport, FunderReportMutation};
use crate::index_client::messages::{IndexClientReport, IndexClientReportMutation};


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
    ResponseSendFunds(ResponseSendFunds),
    /// Reports about current state:
    Report(NodeReport<B,ISA>),
    ReportMutations(Vec<NodeReportMutation<B,ISA>>),
    ResponseRoute(()), // TODO
}

#[allow(unused)]
#[derive(Debug)]
pub enum AppToAppServer<B> {
    /// Set relay address to be used locally (Could be empty)
    SetAddress(Vec<B>), 
    /// Sending funds:
    RequestSendFunds(RequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<Vec<B>>),
    SetFriendAddress(SetFriendAddress<Vec<B>>),
    SetFriendName(SetFriendName),
    RemoveFriend(RemoveFriend),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
    RequestRoute(()), // TODO
    AddIndexServer(()),
    RemoveIndexServer(()),
}
