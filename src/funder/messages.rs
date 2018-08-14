use futures::sync::oneshot;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use networker::messages::{RequestPath};
use database::messages::{ResponseLoadFriends, ResponseLoadFriendToken};
use app_manager::messages::RequestNeighborsRoute;

use proto::funder::{InvoiceId, FunderSendPrice};
use proto::networker::ChannelToken;
use proto::common::SendFundsReceipt;

pub enum FriendStatus {
    Enable = 1,
    Disable = 0,
}

pub enum FriendRequestsStatus {
    Open = 1,
    Close = 0,
}

pub struct FriendLoaded {
    status: FriendStatus,
    requests_status: FriendRequestsStatus,
    wanted_remote_max_debt: u128,
    local_max_debt: u128,
    remote_max_debt: u128,
    balance: i128,
}

pub enum RequestsStatus {
    Open(FunderSendPrice),
    Closed,
}

pub struct FriendUpdated {
    balance: i128,
    local_max_debt: u128,
    remote_max_debt: u128,
    local_pending_debt: u128,
    remote_pending_debt: u128,
    requests_status: RequestsStatus,
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


pub enum ResponseSendFunds {
    Success(SendFundsReceipt),
    Failure(PublicKey), // Reporting public key.
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
    pub mediator_payment_proposal: u64,
    pub invoice_id: InvoiceId,
    pub destination_payment: u128,
}

// ======== Internal interface ========
//
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentProposalPair {
    pub request: FunderSendPrice,
    pub response: FunderSendPrice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriendRouteLink {
    pub node_public_key: PublicKey,
    pub payment_proposal_pair: PaymentProposalPair,
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriendsRoute {
    pub source_public_key: PublicKey,
    pub source_request_proposal: FunderSendPrice,
    pub route_links: Vec<FriendRouteLink>,
    pub dest_public_key: PublicKey,
    pub dest_response_proposal: FunderSendPrice,
}

pub struct FriendsRouteWithCapacity {
    route: FriendsRoute,
    capacity: u128,
}

pub struct RequestSendFunds {
    // Note that it is the sender's responsibility to randomly generate a request_id.
    // This is important to make sure send funds requests can be tracked by the sending
    // application, and can not be lost.
    //
    // TODO: Rename request_id -> payment_id ?
    pub request_id: Uid,
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub payment: u128,
    pub response_sender: oneshot::Sender<ResponseSendFunds>,
}

pub enum FunderToAppManager {
    FriendStateUpdate(FriendStateUpdate),
    RequestNeighborsRoute(RequestNeighborsRoute),
}


pub struct FriendTokenCommon {
    pub friend_public_key: PublicKey,
    pub move_token_message: Vec<u8>,
    // move_token_message is opaque. (Can not be read by the database). 
    // This is why we have the extra old_token and new_token fields.
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(move_token_message)
    pub remote_max_debt: u64,
    pub local_max_debt: u64,
    pub remote_pending_debt: u64,
    pub local_pending_debt: u64,
    pub balance: i64,
    pub local_state: FriendRequestsStatus,
    pub remote_state: FriendRequestsStatus,
}

pub struct InFriendToken {
    pub friend_token_common: FriendTokenCommon,
    pub closed_local_requests: Vec<Uid>,
    pub opened_remote_requests: Vec<PendingFriendRequest>,
}

pub struct OutFriendToken {
    pub friend_token_common: FriendTokenCommon,
    pub opened_local_requests: Vec<PendingFriendRequest>,
    pub closed_remote_requests: Vec<Uid>,
}

#[allow(large_enum_variant)]
pub enum FunderToDatabase {
    StoreFriend(FriendInfo),
    RemoveFriend {
        friend_public_key: PublicKey,
    },
    RequestLoadFriends {
        response_sender: oneshot::Sender<ResponseLoadFriends>,
    },
    StoreInFriendToken(InFriendToken),
    StoreOutFriendToken(OutFriendToken),
    RequestLoadFriendToken {
        friend_public_key: PublicKey,
        response_sender: oneshot::Sender<Option<ResponseLoadFriendToken>>,
    },
}

pub enum FunderToNetworker {
    RequestPath(RequestPath),
}

/*
pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
}
*/
