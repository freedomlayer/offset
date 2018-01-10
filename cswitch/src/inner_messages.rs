#![allow(dead_code, unused)]
extern crate num_bigint;

use std::time::SystemTime;
use std::net::SocketAddr;

use self::num_bigint::BigInt;
use ::crypto::identity::{PublicKey, Signature};
use ::crypto::dh::{Salt, DhPublicKey};
use ::crypto::symmetric_enc::SymmetricKey;
use ::crypto::uid::Uid;
use ::crypto::rand_values::RandValue;


// Helper structs
// --------------

pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;
pub const INDEXING_PROVIDER_ID_LEN: usize = 16;

// A hash of a full link in an indexing provider chain
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct IndexingProviderStateHash([u8; INDEXING_PROVIDER_STATE_HASH_LEN]);

impl IndexingProviderStateHash {
    pub fn from_bytes<T>(t: &T) -> Result<Self, ()>
        where T: AsRef<[u8]>
    {
        let in_bytes = t.as_ref();

        if in_bytes.len() != INDEXING_PROVIDER_STATE_HASH_LEN {
            Err(())
        } else {
            let mut state_hash_bytes = [0; INDEXING_PROVIDER_STATE_HASH_LEN];
            state_hash_bytes.clone_from_slice(in_bytes);
            Ok(IndexingProviderStateHash(state_hash_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for IndexingProviderStateHash {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

// The name of an indexing provider.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct IndexingProviderId([u8; INDEXING_PROVIDER_ID_LEN]);

impl IndexingProviderId {
    pub fn from_bytes<T>(t: &T) -> Result<Self, ()>
        where T: AsRef<[u8]>
    {
        let in_bytes = t.as_ref();

        if in_bytes.len() != INDEXING_PROVIDER_ID_LEN {
            Err(())
        } else {
            let mut provider_id_bytes = [0; INDEXING_PROVIDER_ID_LEN];
            provider_id_bytes.clone_from_slice(in_bytes);
            Ok(IndexingProviderId(provider_id_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for IndexingProviderId {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

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
    max_channels: u32,              // Maximum amount of token channels
    token_channel_capacity: u64,    // Capacity per token channel
}


#[derive(Debug, Eq, PartialEq, Clone)]
pub struct NeighborsRoute {
    pub public_keys: Vec<PublicKey>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FriendsRoute {
    pub public_keys: Vec<PublicKey>,
    // How much credit can we push through this route?
    pub capacity: u64,
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseFriendsRoutes {
    pub routes: Vec<FriendsRoute>,
}

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
    

struct NeighborLoaded {
    address: ChannelerAddress,
    max_channels: u32,
    token_channel_capacity: u64,
    status: NeighborStatus,
}

enum NeighborTokenChannelEventInner {
    Open,
    Close,
    LocalMaxDebtChange(u64),    // Contains new local max debt
    BalanceChange(i64),         // Contains new balance
    InconsistencyError(i64)     // Contains balance required for reset
}

struct NeighborTokenChannelEvent {
    channel_index: u32,
    event: NeighborTokenChannelEventInner,
}

enum NeighborEvent {
    NeighborLoaded(NeighborLoaded),
    TokenChannelEvent(NeighborTokenChannelEvent),
}


struct NeighborStateUpdate {
    neighbor_public_key: PublicKey,
    event: NeighborEvent,
}


enum NetworkerToAppManager {
    MessageReceived(MessageReceived),
    ResponseSendMessage(ResponseSendMessage),
    NeighborStateUpdate(NeighborStateUpdate),
}

// App Manager to Networker
// ---------------------------

enum NeighborStatus {
    Enabled,
    Disabled,
}

enum AppManagerToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
    SetNeighborChannelCapacity {
        neighbor_public_key: PublicKey,
        token_channel_capacity: u64,    // Capacity per token channel
    },
    ResetNeighborChannel {
        neighbor_public_key: PublicKey,
        channel_index: u32,
        // TODO: Should we add wanted parameters for the ChannelReset, or let the Networker use the
        // last Inconsistency message information to perform Reset?
    },
    SetNeighborMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
    AddNeighbor {
        neighbor_info: NeighborInfo,
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    SetNeighborStatus {
        status: NeighborStatus,
    },
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
        destination_node_public_key: PublicKey,
    },
}


// Funder to Indexer Client
// ------------------------

pub struct FriendCapacity {
    send: u64,
    recv: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RequestFriendsRoutes {
    Direct {
        source_node_public_key: PublicKey,
        destination_node_public_key: PublicKey,
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestNeighborsRoutes {
    pub source_node_public_key: PublicKey,
    pub destination_node_public_key: PublicKey,
}

pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseNeighborsRoutes {
    pub routes: Vec<NeighborsRoute>,
}

pub enum IndexerClientToFunder {
    ResponseNeighborsRoute(ResponseNeighborsRoutes)
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct StateChainLink {
    pub previous_state_hash: IndexingProviderStateHash,
    pub new_owners_public_keys: Vec<PublicKey>,
    pub new_indexers_public_keys: Vec<PublicKey>,
    pub signatures_by_old_owners: Vec<Signature>,
}

pub struct IndexingProviderInfo {
    pub id: IndexingProviderId,
    pub state_chain_link: StateChainLink,
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
//
enum FriendStatus {
    Enabled,
    Disabled,
}

enum FriendRequestsStatus {
    Open,
    Close,
}

struct FriendLoaded {
    token_channel_capacity: u128,
    status: FriendStatus,
    requests_status: FriendRequestsStatus,
}

enum FriendEvent {
    Loaded(FriendLoaded),
    Open,
    Close,
    RequestsOpened,
    RequestsClosed,
    LocalMaxDebtChange(u128),   // Contains new local max debt
    BalanceChange(i128),        // Contains new balance
    InconsistencyError(i128),   // Contains balance required for reset
}

struct FriendStateUpdate {
    friend_public_key: PublicKey,
    event: FriendEvent,
}

enum FunderToAppManager {
    FriendStateUpdate(FriendStateUpdate),
    ResponseSendFunds {
        // TODO
    },
}


// App Manager to Funder
// ------------------------

pub struct FriendInfo {
    friend_public_key: PublicKey,
    token_channel_capacity: u128,    // Capacity per token channel
}


enum AppManagerToFunder {
    RequestSendFunds {
        // TODO
        request_id: Uid,
        amount: u128,
        destination_node_public_key: PublicKey,
    },
    ResetFriendChannel {
        // TODO
    },
    AddFriend {
        friend_info: FriendInfo,
    },
    RemoveFriend {
        friend_public_key: PublicKey,
    },
    SetFriendStatus {
        status: FriendStatus,
        requests_status: FriendRequestsStatus,
        // TODO: How to get Ack for change of requests_status?
    },
    SetFriendChannelCapacity {
        friend_public_key: PublicKey,
        token_channel_capacity: u128,
    },
}

pub enum FunderToDatabase {
    // TODO:
}

pub enum DatabaseToFunder {
    // TODO:
}

pub enum NetworkerToDatabase {
    // TODO:
}

pub enum DatabaseToNetworker {
    // TODO:
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
