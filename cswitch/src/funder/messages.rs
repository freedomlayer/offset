use crypto::identity::PublicKey;
use crypto::uuid::Uuid;

use indexer_client::messages::{FriendsRouteWithCapacity, RequestNeighborsRoutes};
use networker::messages::{MessageReceivedResponse, MoveTokenDirection, RequestSendMessage};
                          

use proto::funder::{FriendMoveToken, InvoiceId};

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

pub enum SendFundsResult {
    Success(SendFundsReceipt),
    Failure,
}

pub struct ResponseSendFunds {
    request_id: Uuid,
    result: SendFundsResult,
}

pub struct FriendStateUpdate {
    friend_public_key: PublicKey,
    event: FriendEvent,
}

// TODO: Can we merge this with FriendInfoFromDB
pub struct FriendInfo {
    friend_public_key: PublicKey,
    wanted_remote_max_debt: u128,
}

pub struct PendingFriendRequest {
    pub request_id: Uuid,
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
    request_id: Uuid,
    route: FriendsRoute,
    invoice_id: InvoiceId,
    payment: u128,
}

pub enum FunderToAppManager {
    FriendStateUpdate(FriendStateUpdate),
    ResponseSendFunds(ResponseSendFunds),
}

// TODO:
pub enum FunderToDatabase {
    StoreFriend {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
        status: FriendStatus,
    },
    RemoveFriend {},
    RequestLoadFriends {},
    StoreInFriendToken {},
    StoreOutFriendToken {},
    RequestLoadFriendToken {},
}

pub enum FunderToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseSendFunds(ResponseSendFunds),
}

pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
}
