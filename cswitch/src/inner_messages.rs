#![allow(dead_code, unused)]
extern crate num_bigint;

use std::time::SystemTime;
use std::net::SocketAddr;

use self::num_bigint::BigInt;
use utils::crypto::identity::{PublicKey, Signature};
use utils::crypto::dh::{Salt, DhPublicKey};
use utils::crypto::sym_encrypt::SymmetricKey;
use utils::crypto::uuid::Uuid;
use utils::crypto::rand_values::RandValue;


// Helper structs
// --------------

// Networker to Channeler
// ----------------------

pub enum ServerType {
    PublicServer,
    PrivateServer
}

// Networker interface
// -------------------

/// The result of attempting to send a message to a remote Networker.
pub enum SendMessageResult {
    Success(Vec<u8>),
    Failure,
}

/// Destination port for the packet.
/// The destination port is used by the destination Networker to know where to forward the received
/// message.
pub enum DestPort {
    Funder,
    IndexerClient,
    AppManager(u32),
}

/// Component -> Networker
pub struct RequestSendMessage {
    request_id: Uuid,
    route: NeighborsRoute,
    dest_port: DestPort,
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

/// Networker -> Component
pub struct ResponseSendMessage {
    request_id: Uuid,
    result: SendMessageResult,
}

/// Networker -> Component
pub struct MessageReceived {
    request_id: Uuid,
    route: NeighborsRoute, // sender_public_key is the first public key on the NeighborsRoute
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

/// Component -> Networker
pub struct RespondMessageReceived {
    request_id: Uuid,
    response_data: Vec<u8>,
}

/// Component -> Networker
pub struct DiscardMessageReceived {
    request_id: Uuid,
}


// Indexer client to Networker
// ---------------------------

pub enum IndexerClientToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
    ResponseMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
}

// Networker to Indexer client
// ---------------------------

pub enum NetworkerToIndexerClient {
    ResponseSendMessage(ResponseSendMessage),
    MessageReceived(MessageReceived),
    RequestFriendsRoutes(RequestFriendsRoutes),
}


// Networker to App Manager
// ---------------------------


enum NetworkerToAppManager {
    SendMessageRequestReceived {
        request_id: Uuid,
        source_node_public_key: PublicKey,
        request_content: Vec<u8>,
        max_response_length: u64,
        processing_fee: u64,
    },
    InvalidNeighborMoveToken {
        // TODO
    },
    NeighborsState {
        // TODO: Current state of neighbors.

    },
    NeighborsUpdates {
        // TODO: Add some summary of information about neighbors and their token channels.
        // Possibly some counters?
    },
}

// App Manager to Networker
// ---------------------------


enum AppManagerToNetworker {
    RespondSendMessageRequest {
        request_id: Uuid,
        response_content: Vec<u8>,
    },
    DiscardSendMessageRequest {
        request_id: Uuid,
    },
    SetNeighborChannelCapacity {
        token_channel_capacity: u64,    // Capacity per token channel
    },
    SetNeighborMaxChannels {
        max_channels: u32,
    },
    AddNeighbor {
        neighbor_info: NeighborInfo,
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    SetServerType(ServerType),
}


// Funder to Networker
// -------------------

enum ResponseSendFundsStatus {
    Success,
    Failure,
}

enum FunderToNetworker {
    FundsReceived {
        source_node_public_key: PublicKey,
        amount: u64,
        message_content: Vec<u8>,
    },
    ResponseSendFunds {
        request_id: Uuid,
        status: ResponseSendFundsStatus,
    }
}


enum NetworkerToFunder {
    MessageReceived {
        source_node_public_key: PublicKey,
        message_content: Vec<u8>,
    },
    RequestSendFunds {
        request_id: Uuid,
        amount: u64,
        message_content: Vec<u8>,
        destination_node_public_key: PublicKey,
    },
}


// Funder to Indexer Client
// ------------------------

pub struct FriendCapacity {
    send: u64,
    recv: u64,
}


pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
}


pub enum IndexerClientToFunder {
    ResponseNeighborsRoute(ResponseNeighborsRoutes)
}

pub enum AppManagerToIndexerClient {
    AddIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider {
        id: IndexingProviderId,
    },
    RequestNeighborsRoutes(RequestNeighborsRoutes),
    RequestFriendsRoutes(RequestFriendsRoutes),
}

pub enum IndexingProviderStateUpdate {
    Add(IndexingProviderInfo),
    Remove(IndexingProviderId),
}

pub enum IndexerClientToAppManager {
    IndexingProviderStateUpdate(IndexingProviderStateUpdate),
    ResponseNeighborsRoutes(ResponseNeighborsRoutes),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
}

pub enum IndexerClientToDatabase {
    StoreIndexingProvider(IndexingProviderInfo),
    RequestLoadIndexingProvider,
    StoreRoute {
        id: IndexingProviderId,
        route: NeighborsRoute,
    },
}

pub struct IndexingProviderInfoFromDB {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    last_routes: Vec<NeighborsRoute>,
}

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfoFromDB>)
}

// Funder to App Manager
// ------------------------

// TODO: Not done here:

enum ResponseAddFriendStatus {
    Success,
    FailureFriendAlreadyExist,

}

enum ResponseRemoveFriendStatus {
    Success,
    FailureFriendNonexistent,
    FailureMutualCreditNonzero,
}

enum ResponseSetFriendCapacityStatus {
    Success,
    FailureFriendNonexistent,
    FailureProposedCapacityTooSmall,
}

enum ResponseOpenFriendStatus {
    Success,
    FailureFriendNonexistent,
    FailureFriendAlreadyOpen,
}

enum ResponseCloseFriendStatus {
    Success,
    FailureFriendNonexistent,
    FailureFriendAlreadyClosed,
}

enum FunderToAppManager {
    FundsReceived {
        source_node_public_key: PublicKey,
        amount: u64,
        message_content: Vec<u8>,
    },
    InvalidFriendMoveToken {
        // TODO
    },
    ResponseSendFunds {
        request_id: Uuid,
        status: ResponseSendFundsStatus,
    },
    ResponseAddFriend {
        request_id: Uuid,
        status: ResponseAddFriendStatus,
    },
    ResponseRemoveFriend {
        request_id: Uuid,
        status: ResponseRemoveFriendStatus,
    },
    ResponseSetFriendCapacity {
        request_id: Uuid,
        status: ResponseSetFriendCapacityStatus,
    },
    ResponseOpenFriend {
        request_id: Uuid,
        status: ResponseOpenFriendStatus,
    },
    ResponseCloseFriend {
        request_id: Uuid,
        status: ResponseCloseFriendStatus,
    },
    FriendsState {
        // TODO: Current state of friends.

    },
    FriendsUpdates {
        // TODO: Add some summary of information about friends and their token channels.
        // Possibly some counters?
    },
}


// App Manager to Funder
// ------------------------

enum AppManagerToFunder {
    RequestSendFunds {
        request_id: Uuid,
        amount: u64,
        message_content: Vec<u8>,
        destination_node_public_key: PublicKey,
    },
    RequestAddFriend {
        request_id: Uuid,
        friend_public_key: PublicKey,
        capacity: BigInt, // Max debt possible
    },
    RequestRemoveFriend {
        request_id: Uuid,
        friend_public_key: PublicKey,
    },
    RequestSetFriendCapacity {
        request_id: Uuid,
        friend_public_key: PublicKey,
        new_capacity: BigInt,
    },
    RequestOpenFriend {
        request_id: Uuid,
        friend_public_key: PublicKey,
    },
    RequestCloseFriend {
        request_id: Uuid,
        friend_public_key: PublicKey,
    }
}

pub enum FunderToDatabase {
    // TODO:
}

pub enum DatabaseToFunder {
    // TODO:
}

pub struct StoreNeighborInfo {
    pub neighbor_public_key: PublicKey,
    pub remote_maximum_debt: u64,
    pub maximum_channel: u32,
    pub is_enable: bool
}

pub struct MoveTokenMessage {
    pub move_token_transactions: Bytes,
    // pub move_token_old_token: TODO
    pub move_tolen_rand_nonce: RandValue,
}

pub enum NetworkerToDatabase {
    StoreNeighbor(StoreNeighborInfo),
    RemoveNeighbor {
        neighbor_public_key: PublicKey
    },
    RequestLoadNeighbors,
    StoreInNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
        move_token_message: MoveTokenMessage,
        remote_maximum_debt: u64,
        local_maximum_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: u64,
        local_funds_rand_nonce: Option<RandValue>,
        remote_funds_rand_nonce: Option<RandValue>,
        closed_local_requests: Vec<Uuid>,
    }
}

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors,
}
