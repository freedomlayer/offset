use crate::funder::messages::{RequestSendFunds, ResponseSendFunds};
use crate::report::messages::{FunderReport, FunderReportMutation};

#[allow(unused)]
enum AppServerToApp<A> {
    /// Funds:
    ResponseSendFunds(ResponseSendFunds),
    /// Reports about current state:
    Report(FunderReport<A>),
    ReportMutations(Vec<FunderReportMutation<A>>),
    /// Response for delegate request:
    ResponseDelegate(()),
}

#[allow(unused)]
enum AppToAppServer<A> {
    /// Set relay address to be used locally (Could be empty)
    SetAddress(Option<A>), 
    /// Sending funds:
    RequestSendFunds(RequestSendFunds),
    ReceiptAck(()), // TODO
    /// Friend management:
    AddFriend(()), // TODO
    SetFriendInfo(()),
    RemoveFriend(()),
    EnableFriend(()),
    DisableFriend(()),
    OpenFriend(()),
    CloseFriend(()),
    SetFriendRemoteMaxDebt(()),
    ResetFriendChannel(()),
    /// Delegation:
    RequestDelegate(()),
}
