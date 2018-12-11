use crypto::identity::PublicKey;

use crate::funder::messages::{RequestSendFunds, ResponseSendFunds,
                            ReceiptAck, AddFriend, SetFriendInfo, RemoveFriend,
                            SetFriendRemoteMaxDebt, ResetFriendChannel};
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
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<A>),
    SetFriendInfo(SetFriendInfo<A>),
    RemoveFriend(RemoveFriend),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
    /// Delegation:
    RequestDelegate(()),
}
