use futures::sync::oneshot;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use indexer_client::messages::{FriendsRouteWithCapacity, RequestNeighborsRoutes};
use networker::messages::{RequestPath};
use database::messages::{ResponseLoadFriends, ResponseLoadFriendToken};

use proto::funder::{InvoiceId, FriendMoveToken};
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

enum FriendEvent {
    Loaded(FriendLoaded),
    Open,
    Close,
    RequestsOpened,
    RequestsClosed,
    LocalMaxDebtChange(u128),  // Contains new local max debt
    RemoteMaxDebtChange(u128), // Contains new local max debt
    BalanceChange(i128),       // Contains new balance
    InconsistencyError(i128),  // Contains balance required for reset
}


pub enum ResponseSendFunds {
    Success(SendFundsReceipt),
    Failure,
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
    pub route: FriendsRouteWithCapacity,
    pub mediator_payment_proposal: u64,
    pub invoice_id: InvoiceId,
    pub destination_payment: u128,
}

// ======== Internal interface ========

pub struct FriendsRoute {
    public_keys: Vec<PublicKey>,
}

pub struct RequestSendFunds {
    pub route: FriendsRoute,
    pub invoice_id: InvoiceId,
    pub payment: u128,
    pub response_sender: oneshot::Sender<ResponseSendFunds>,
}

pub enum FunderToAppManager {
    FriendStateUpdate(FriendStateUpdate),
}


pub struct FriendTokenCommon {
    pub friend_public_key: PublicKey,
    pub move_token_message: FriendMoveToken,
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

pub enum FunderToDatabase {
    StoreFriend {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
        status: FriendStatus,
    },
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

pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
}
