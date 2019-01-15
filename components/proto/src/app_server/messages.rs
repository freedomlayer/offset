use crypto::identity::{PublicKey, Signature};
use crypto::crypto_rand::RandValue;

use crate::funder::messages::{RequestSendFunds, ResponseSendFunds,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, RemoveFriend,
                            SetFriendRemoteMaxDebt, ResetFriendChannel};
use crate::funder::report::{FunderReport, FunderReportMutation};

#[allow(unused)]
#[derive(Debug)]
pub struct RequestDelegate {
    pub app_rand_nonce: RandValue,
}

#[allow(unused)]
#[derive(Debug)]
pub struct ResponseDelegate {
    pub app_public_key: PublicKey,
    pub app_rand_nonce: RandValue,
    pub server_rand_nonce: RandValue,
    pub signature: Signature,
    // sha512/256(sha512/256("DELEGATE") ||
    //               appPublicKey ||
    //               appRandNonce ||
    //               serverRandNonce)
}

#[allow(unused)]
#[derive(Debug)]
pub enum AppServerToApp<A: Clone> {
    /// Funds:
    ResponseSendFunds(ResponseSendFunds),
    /// Reports about current state:
    Report(FunderReport<A>),
    ReportMutations(Vec<FunderReportMutation<A>>),
    /// Response for delegate request:
    ResponseDelegate(ResponseDelegate),
}

#[allow(unused)]
#[derive(Debug)]
pub enum AppToAppServer<A> {
    /// Set relay address to be used locally (Could be empty)
    SetAddress(Option<A>), 
    /// Sending funds:
    RequestSendFunds(RequestSendFunds),
    ReceiptAck(ReceiptAck),
    /// Friend management:
    AddFriend(AddFriend<A>),
    SetFriendAddress(SetFriendAddress<A>),
    SetFriendName(SetFriendName),
    RemoveFriend(RemoveFriend),
    EnableFriend(PublicKey),
    DisableFriend(PublicKey),
    OpenFriend(PublicKey),
    CloseFriend(PublicKey),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
    /// Delegation:
    RequestDelegate(RequestDelegate),
}
