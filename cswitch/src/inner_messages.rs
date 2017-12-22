#![allow(dead_code, unused)]
extern crate num_bigint;

use std::time::SystemTime;
use std::net::SocketAddr;

use self::num_bigint::BigInt;
use ::crypto::identity::{PublicKey, Signature};
use ::crypto::dh::Salt;
use ::crypto::symmetric_enc::SymmetricKey;
use ::crypto::uid::Uid;
use ::crypto::rand_values::RandValue;

use ::networker::networker_client::NetworkerRespondableRequest;


// Helper structs
// --------------

const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;
const INDEXING_PROVIDER_NAME_LEN: usize = 16;

// A hash of a full link in an indexing provider chain
struct IndexingProviderStateHash([u8; INDEXING_PROVIDER_STATE_HASH_LEN]);

// The name of an indexing provider.
// TODO: Should we use a string instead here? Is a fixed sized blob preferable?
pub struct IndexingProviderName([u8; INDEXING_PROVIDER_NAME_LEN]);


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

struct NeighborTokenChannel {
    mutual_credit: i64, // TODO: Is this the right type?
    // Possibly change to bignum?

    // TODO:
    // - A type for last token
    // - Keep last operation?

}

struct NeighborTokenChannelInfo {
    neighbor_info: NeighborInfo,
    neighbor_token_channels: Vec<NeighborTokenChannel>,
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

/*
struct ResponseNeighborsRelationList {
    neighbors_relation_list: Vec<NeighborInfo>,
}
*/

pub enum ChannelerToNetworker {
    ChannelOpened(ChannelOpened),
    ChannelClosed(ChannelClosed),
    ChannelMessageReceived(ChannelMessageReceived),
    // ResponseNeighborsRelationList(ResponseNeighborsRelationList),
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
    /*
    CloseChannel {
        neighbor_public_key: PublicKey,
    },
    */
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
    // RequestNeighborsRelationList,
    // SetServerType(ServerType),
}

// Indexer client to Networker
// ---------------------------


enum IndexerClientToNetworker {
    RequestSendMessage {
        request_id: Uid,
        request_content: Vec<u8>,
        neighbors_route: NeighborsRoute,
        max_response_length: u64,
        processing_fee: u64,
        delivery_fee: u64,
        dest_node_public_key: PublicKey,
    },
    IndexerAnnounce {
        content: Vec<u8>,
    },
    ResponseFriendsRoute {
        routes: Vec<FriendsRoute>,
        dest_comm_public_key: PublicKey,
        dest_recent_timestamp: RandValue,
    },
}

// Networker to Indexer client
// ---------------------------

enum ResponseSendMessageContent {
    Success(Vec<u8>),
    Failure,
}

pub enum NotifyStructureChangeNeighbors {
    NeighborAdded(PublicKey),
    NeighborRemoved(PublicKey),
    TimestampUpdated(RandValue),
    CommPublicKeyUpdated(PublicKey),
}

pub enum NetworkerToIndexerClient<R> {
    /*
    ResponseSendMessage {
        request_id: Uid,
        content: ResponseSendMessageContent,
    },
    */
    NotifyStructureChange(NotifyStructureChangeNeighbors),
    RequestReceived(NetworkerRespondableRequest<R>),
    RequestFriendsRoute(RequestFriendsRoute),

    /*
    MessageReceived {
        source_node_public_key: PublicKey,
        message_content: Vec<u8>,
    },
    */
}


// Networker to Plugin Manager
// ---------------------------


enum NetworkerToPluginManager {
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

// Plugin Manager to Networker
// ---------------------------


enum PluginManagerToNetworker {
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
    IndexerAnnounceSelf {
        current_time: u64,      // Current perceived time
        owner_public_key: PublicKey, // Public key of the signing owner.
        owner_sign_time: u64,   // The time in which the owner has signed
        owner_signature: Signature, // The signature of the owner over (owner_sign_time || indexer_public_key)
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


// Networker to Funder
// -------------------

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


// Funder to Indexer Client
// ------------------------

pub struct FriendCapacity {
    send: u64,
    recv: u64,
}

pub enum NotifyStructureChangeFriends {
    // This message is used both to add a new friend and to update the capacity information of a
    // current friend.
    FriendUpdated {
        public_key: PublicKey,
        capacity: FriendCapacity
    },
    FriendRemoved(PublicKey),
    TimestampUpdated(RandValue),
    CommPublicKeyUpdated(PublicKey),
}

pub enum RequestFriendsRoute {
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

pub enum FunderToIndexerClient {
    RequestNeighborsRoute {
        source_node_public_key: PublicKey,
        dest_node_public_key: PublicKey,
    },
    NotifyStructureChange(NotifyStructureChangeFriends),
}


// Indexer Client to Funder
// ------------------------

pub enum IndexerClientToFunder {
    ResponseNeighborsRoute {
        routes: Vec<NeighborsRoute>,
        dest_comm_public_key: PublicKey,
        dest_recent_timestamp: RandValue,
    }
}

pub struct IndexingProviderInfo {
    name: IndexingProviderName,
    previous_state_hash: IndexingProviderStateHash,
    new_owners_public_keys: Vec<PublicKey>,
    new_indexers_public_keys: Vec<PublicKey>,
    signatures_by_old_owners: Vec<Signature>,
}

pub enum PluginManagerToIndexerClient {
    AddIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider {
        name: IndexingProviderName,
    },
}


pub enum IndexerClientToPluginManager {
    IndexingProviderUpdated(IndexingProviderInfo),
}


// Funder to Plugin Manager
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

enum FunderToPluginManager {
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


// Plugin Manager to Funder
// ------------------------

enum PluginManagerToFunder {
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


// TODO: Should pather report about closed path?
// How to do this reliably?

// Security Module
// ---------------


pub enum FromSecurityModule {
    ResponseSign {
        signature: Signature,
    },
    /*
    ResponseVerify {
        request_id: Uid,
        result: bool,
    },
    */
    ResponsePublicKey {
        public_key: PublicKey,
    },
    /*
    ResponseSymmetricKey {
        request_id: Uid,
        symmetric_key: SymmetricKey,
    },
    */
}

pub enum ToSecurityModule {
    RequestSign {
        message: Vec<u8>,
    },
    /*
    RequestVerify {
        request_id: Uid,
        message: Vec<u8>,
        public_key: PublicKey,
        signature: Signature,
    },
    */
    RequestPublicKey {
    },
    /*
    RequestSymmetricKey {
        request_id: Uid,
        public_key: PublicKey,
        salt: Salt,
    },
    */
}

// Timer
// -----

pub enum FromTimer {
    TimeTick,
}
