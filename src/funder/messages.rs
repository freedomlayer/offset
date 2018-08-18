use futures::sync::oneshot;
use bytes::Bytes;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use proto::funder::{ChannelToken, InvoiceId};
use proto::common::SendFundsReceipt;
use channeler::types::ChannelerNeighborInfo;
use super::types::{RequestsStatus, FriendStatus};
use super::handler::handle_control::{SetFriendRemoteMaxDebt, ResetFriendChannel,
    SetFriendAddr, AddFriend, RemoveFriend, SetFriendStatus, SetRequestsStatus, 
    RequestSendFunds, ReceiptAck};


pub struct FriendUpdated {
    balance: i128,
    local_max_debt: u128,
    remote_max_debt: u128,
    local_pending_debt: u128,
    remote_pending_debt: u128,
    requests_status: RequestsStatus,
    status: FriendStatus,
}

pub struct FriendInconsistent {
    current_token: ChannelToken,
    balance_for_reset: i128,
}

pub enum FriendEvent {
    FriendUpdated(FriendUpdated),
    FriendRemoved,
    FriendInconsistent(FriendInconsistent),
}

pub enum ResponseSendFundsResult {
    Success(SendFundsReceipt),
    Failure(PublicKey), // Reporting public key.
}

pub struct CtrlResponseSendFunds {
    pub result: ResponseSendFundsResult,
    pub ack_sender: oneshot::Sender<()>,
}

pub struct FriendStateUpdate {
    friend_public_key: PublicKey,
    event: FriendEvent,
}

// TODO: Can we merge this with FriendInfoFromDB
pub struct FriendInfo {
    friend_public_key: PublicKey,
    wanted_remote_max_debt: u128,
    status: FriendStatus,
}

pub struct PendingFriendRequest {
    pub request_id: Uid,
    // pub route: FriendsRouteWithCapacity, // TODO: Fill in later
    pub invoice_id: InvoiceId,
    pub destination_payment: u128,
}

// ======== Internal interface ========

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriendsRoute {
    pub route_links: Vec<PublicKey>,
}

pub struct FriendsRouteWithCapacity {
    route: FriendsRoute,
    capacity: u128,
}


pub struct CtrlRequestSendFunds {
    // Note that it is the sender's responsibility to randomly generate a request_id.
    // This is important to make sure send funds requests can be tracked by the sending
    // application, and can not be lost.
    //
    // TODO: Rename request_id -> payment_id ?
    request_send_funds: RequestSendFunds,
    pub response_sender: oneshot::Sender<CtrlResponseSendFunds>,
}



pub enum FunderToDatabase {
    // TODO
}


pub enum FunderToChanneler<A> {
    /// Request send message to remote.
    SendChannelMessage {
        friend_public_key: PublicKey,
        content: Bytes,
    },
    /// Request to add a new friend.
    AddFriend {
        info: ChannelerNeighborInfo<A>,
    },
    /// Request to remove a friend.
    RemoveFriend {
        friend_public_key: PublicKey
    },
}



pub enum FunderCommand<A> {
    AddFriend(AddFriend<A>),
    RemoveFriend(RemoveFriend),
    SetRequestsStatus(SetRequestsStatus),
    SetFriendStatus(SetFriendStatus),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    SetFriendAddr(SetFriendAddr<A>),
    ResetFriendChannel(ResetFriendChannel),
    RequestSendFunds(CtrlRequestSendFunds),
}
