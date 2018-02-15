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
    StoreInFriendToken {
        friend_public_key: PublicKey,
        token_channel_index: u32,
        move_token_message: FriendMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_state: FriendRequestsStatus,
        remote_state: FriendRequestsStatus,
        closed_local_requests: Vec<Uid>,
        opened_remote_requests: Vec<PendingFriendRequest>,
    },
    StoreOutFriendToken {
        friend_public_key: PublicKey,
        move_token_message: FriendMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        opened_local_requests: Vec<PendingFriendRequest>,
        closed_remote_requests: Vec<Uid>,
    },
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
