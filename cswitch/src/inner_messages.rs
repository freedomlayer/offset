#![allow(dead_code, unused)]
extern crate num_bigint;

use std::time::SystemTime;
use std::net::SocketAddr;

use self::num_bigint::BigInt;
use ::crypto::identity::{PublicKey, Signature};
use ::crypto::dh::DhPublicKey;
use ::crypto::dh::Salt;
use ::crypto::symmetric_enc::SymmetricKey;
use ::crypto::uid::Uid;
use ::crypto::rand_values::RandValue;


// Helper structs
// --------------

const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;
const INDEXING_PROVIDER_NAME_LEN: usize = 16;

// A hash of a full link in an indexing provider chain
struct IndexingProviderStateHash([u8; INDEXING_PROVIDER_STATE_HASH_LEN]);

// The name of an indexing provider.
// TODO: Should we use a string instead here? Is a fixed sized blob preferable?
struct IndexingProviderId([u8; INDEXING_PROVIDER_NAME_LEN]);


#[derive(Clone, Debug)]
pub struct ChannelerAddress {
    pub socket_addr: Option<SocketAddr>,
    pub neighbor_public_key: PublicKey,
}

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo {
    pub neighbor_address: ChannelerAddress,
    pub max_channels: u32,  // Maximum amount of token channels
}

pub struct NeighborInfo {
    neighbor_public_key: PublicKey,
    neighbor_address: ChannelerAddress,
    max_channels: u32,  // Maximum amount of token channels
    token_channel_capacity: u64,    // Capacity per token channel
}


pub struct NeighborsRoute {
    route: Vec<u32>,
}

struct FriendsRoute {
    route: Vec<u32>,
    // How much credit can we push through this route?
    capacity: u64,
}



// Channeler to Networker
// ----------------------

pub struct ChannelOpened {
    pub remote_public_key: PublicKey, // Public key of remote side
    pub locally_initialized: bool, // Was this channel initiated by this end.
}

pub struct ChannelClosed {
    pub remote_public_key: PublicKey,
}

pub struct ChannelMessageReceived {
    pub remote_public_key: PublicKey,
    pub message_content: Vec<u8>,
}

pub enum ChannelerToNetworker {
    ChannelOpened(ChannelOpened),
    ChannelClosed(ChannelClosed),
    ChannelMessageReceived(ChannelMessageReceived),
}


// Networker to Channeler
// ----------------------

pub enum ServerType {
    PublicServer,
    PrivateServer
}

pub enum NetworkerToChanneler {
    SendChannelMessage {
        token: u32,
        neighbor_public_key: PublicKey,
        content: Vec<u8>,
    },
    AddNeighbor {
        neighbor_info: ChannelerNeighborInfo,
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    SetMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
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
    request_id: Uid,
    route: NeighborsRoute,
    dest_port: DestPort,
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

/// Networker -> Component
pub struct ResponseSendMessage {
    request_id: Uid,
    result: SendMessageResult,
}

/// Networker -> Component
pub struct MessageReceived {
    request_id: Uid,
    route: NeighborsRoute, // sender_public_key is the first public key on the NeighborsRoute
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    half_credits_per_byte_proposal: u32,
}

/// Component -> Networker
pub struct RespondMessageReceived {
    request_id: Uid,
    response_data: Vec<u8>,
}

/// Component -> Networker
pub struct DiscardMessageReceived {
    request_id: Uid,
}


// Indexer client to Networker
// ---------------------------


enum IndexerClientToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
    ResponseMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
}

// Networker to Indexer client
// ---------------------------


enum NetworkerToIndexerClient {
    ResponseSendMessage(ResponseSendMessage),
    MessageReceived(MessageReceived),
    RequestFriendsRoutes(RequestFriendsRoutes),
}


// Networker to App Manager
// ---------------------------


enum NetworkerToAppManager {
    SendMessageRequestReceived {
        request_id: Uid,
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
        request_id: Uid,
        response_content: Vec<u8>,
    },
    DiscardSendMessageRequest {
        request_id: Uid,
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
        request_id: Uid,
        status: ResponseSendFundsStatus,
    }
}


enum NetworkerToFunder {
    MessageReceived {
        source_node_public_key: PublicKey,
        message_content: Vec<u8>,
    },
    RequestSendFunds {
        request_id: Uid,
        amount: u64,
        message_content: Vec<u8>,
        dest_node_public_key: PublicKey,
    },
}


struct FriendCapacity {
    send: u64,
    recv: u64,
}

enum RequestFriendsRoutes {
    Direct {
        source_node_public_key: PublicKey,
        dest_node_public_key: PublicKey,
    },
    LoopFromFriend {
        // A loop from myself through given friend, back to myself.
        // This is used for money rebalance when we owe the friend money.
        friend_public_key: PublicKey,
    },
    LoopToFriend {
        // A loop from myself back to myself through given friend.
        // This is used for money rebalance when the friend owe us money.
        friend_public_key: PublicKey,
    },

}

struct ResponseFriendsRoutes {
    routes: Vec<FriendsRoute>,
}

struct RequestNeighborsRoutes {
    source_node_public_key: PublicKey,
    dest_node_public_key: PublicKey,
}

struct ResponseNeighborsRoutes {
    routes: Vec<NeighborsRoute>,
}


enum FunderToIndexerClient {
    RequestNeighborsRoutes(RequestNeighborsRoutes),
}


enum IndexerClientToFunder {
    ResponseNeighborsRoutes(ResponseNeighborsRoutes),
}

struct StateChainLink {
    previous_state_hash: IndexingProviderStateHash,
    new_owners_public_keys: Vec<PublicKey>,
    new_indexers_public_keys: Vec<PublicKey>,
    signatures_by_old_owners: Vec<Signature>,
}

struct IndexingProviderInfo {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
}

enum AppManagerToIndexerClient {
    AddIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider {
        name: IndexingProviderId,
    },
    RequestNeighborsRoutes(RequestNeighborsRoutes),
    RequestFriendsRoutes(RequestFriendsRoutes),
}

enum IndexingProviderStateUpdate {
    Add(IndexingProviderInfo),
    Remove(IndexingProviderId),
}

enum IndexerClientToAppManager {
    IndexingProviderStateUpdate(IndexingProviderStateUpdate),
    ResponseNeighborsRoutes(ResponseNeighborsRoutes),
    ResponseFriendsRoutes(ResponseFriendsRoutes), 
}

enum IndexerClientToDatabase {
    StoreIndexingProvider(IndexingProviderInfo),
    RequestLoadIndexingProvider,
}

enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfo>)
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
        request_id: Uid,
        status: ResponseSendFundsStatus,
    },
    ResponseAddFriend {
        request_id: Uid,
        status: ResponseAddFriendStatus,
    },
    ResponseRemoveFriend {
        request_id: Uid,
        status: ResponseRemoveFriendStatus,
    },
    ResponseSetFriendCapacity {
        request_id: Uid,
        status: ResponseSetFriendCapacityStatus,
    },
    ResponseOpenFriend {
        request_id: Uid,
        status: ResponseOpenFriendStatus,
    },
    ResponseCloseFriend {
        request_id: Uid,
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
        request_id: Uid,
        amount: u64,
        message_content: Vec<u8>,
        dest_node_public_key: PublicKey,
    },
    RequestAddFriend {
        request_id: Uid,
        friend_public_key: PublicKey,
        capacity: BigInt, // Max debt possible
    },
    RequestRemoveFriend {
        request_id: Uid,
        friend_public_key: PublicKey,
    },
    RequestSetFriendCapacity {
        request_id: Uid,
        friend_public_key: PublicKey,
        new_capacity: BigInt,
    },
    RequestOpenFriend {
        request_id: Uid,
        friend_public_key: PublicKey,
    },
    RequestCloseFriend {
        request_id: Uid,
        friend_public_key: PublicKey,
    }
}



// Security Module
// ---------------


pub enum FromSecurityModule {
    ResponseSign {
        signature: Signature,
    },
    ResponsePublicKey {
        public_key: PublicKey,
    },
}

pub enum ToSecurityModule {
    RequestSign {
        message: Vec<u8>,
    },
    RequestPublicKey {
    },
}

// Timer
// -----

pub enum FromTimer {
    TimeTick,
}
