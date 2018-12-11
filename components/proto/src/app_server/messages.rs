use crate::funder::messages::{RequestSendFunds, ResponseSendFunds};

#[allow(unused)]
enum AppServerToApp {
    /// Funds:
    ResponseSendFunds(ResponseSendFunds),
    /// Reports about current state:
    Report(()),
    ReportMutations(()),
    /// REsponse for delegate request:
    ResponseDelegate(()),
}

#[allow(unused)]
enum AppToAppServer {
    /// Set relay address to be used locally (Could be empty)
    SetAddress(()),
    /// Sending funds:
    RequestSendFunds(RequestSendFunds),
    ReceiptAck(()),
    /// Friend management:
    AddFriend(()),
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
