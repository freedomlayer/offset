use crypto::identity::{PublicKey, Signature};
use crypto::crypto_rand::RandValue;

use crate::funder::messages::{RequestSendFunds, ResponseSendFunds,
                            ReceiptAck, AddFriend, SetFriendAddress, 
                            SetFriendName, RemoveFriend,
                            SetFriendRemoteMaxDebt, ResetFriendChannel,
                            TPublicKey};
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
// #[derive()]
pub enum AppServerToApp<A:Clone,P:Clone,MS:Clone,RS> {
    /// Funds:
    ResponseSendFunds(ResponseSendFunds<RS>),
    /// Reports about current state:
    Report(FunderReport<A,P,MS>),
    ReportMutations(Vec<FunderReportMutation<A,P,MS>>),
    /// Response for delegate request:
    ResponseDelegate(ResponseDelegate),
}

#[allow(unused)]
#[derive(Debug)]
pub enum AppToAppServer<A,P,RS,MS> {
    /// Set relay address to be used locally (Could be empty)
    SetAddress(Option<A>), 
    /// Sending funds:
    RequestSendFunds(RequestSendFunds<P>),
    ReceiptAck(ReceiptAck<RS>),
    /// Friend management:
    AddFriend(AddFriend<A,P>),
    SetFriendAddress(SetFriendAddress<A,P>),
    SetFriendName(SetFriendName<P>),
    RemoveFriend(RemoveFriend<P>),
    EnableFriend(TPublicKey<P>),
    DisableFriend(TPublicKey<P>),
    OpenFriend(TPublicKey<P>),
    CloseFriend(TPublicKey<P>),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt<P>),
    ResetFriendChannel(ResetFriendChannel<P,MS>),
    /// Delegation:
    RequestDelegate(RequestDelegate),
}
