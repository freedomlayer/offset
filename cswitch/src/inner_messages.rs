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
pub const RECEIPT_RESPONSE_HASH_LEN: usize = 32;
pub const INDEXING_PROVIDER_ID_LEN: usize = 16;
pub const INVOICE_ID_LEN: usize = 32;
pub const CHANNEL_TOKEN_LEN: usize = 32;
pub const HASH_RESULT_LEN: usize = 32;

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

// The Id of an indexing provider.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct IndexingProviderId([u8; INDEXING_PROVIDER_ID_LEN]);

/// The Id number of an invoice. An invoice is used during payment through the Funder. 
/// It is chosen by the sender of funds. The invoice id then shows up in the receipt for the
/// payment.
pub struct InvoiceId([u8; INVOICE_ID_LEN]);

/// A hash of:
/// = sha512/256(requestId || 
///       sha512/256(nodeIdPath) || 
///       mediatorPaymentProposal)
/// Used inside SendFundsReceipt
struct ReceiptResponseHash([u8; RECEIPT_RESPONSE_HASH_LEN]);


/// The hash of the previous message sent over the token channel.
struct ChannelToken([u8; CHANNEL_TOKEN_LEN]);

/// A Sha512/256 hash over some buffer.  
/// TODO: Move this into a separate crypto/hash.rs file,
/// together with Sha512/256 implementation.
struct HashResult([u8; HASH_RESULT_LEN]);

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


#[derive(Debug, Eq, PartialEq, Clone)]
pub struct NeighborsRoute {
    pub public_keys: Vec<PublicKey>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FriendsRouteWithCapacity {
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
    credits_per_byte_proposal: u32,
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
    credits_per_byte_proposal: u32,
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


// Funder interface
// ----------------

struct FriendsRoute {
    public_keys: Vec<PublicKey>,
}

struct RequestSendFunds {
    request_id: Uid,
    route: FriendsRoute,
    invoice_id: InvoiceId,
    payment: u128,
}


/// A SendFundsReceipt is received if a RequestSendFunds is successful.
/// It can be used a proof of payment for a specific invoice_id.
struct SendFundsReceipt {
    response_hash: ReceiptResponseHash,
    // = sha512/256(requestId || 
    //       sha512/256(nodeIdPath) || 
    //       mediatorPaymentProposal)
    invoice_id: InvoiceId,
    payment: u128,
    rand_nonce: RandValue,
    signature: Signature,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
    //   invoiceId ||
    //   payment ||
    //   randNonce)
}

enum SendFundsResult {
    Success(SendFundsReceipt),
    Failure,
}

struct ResponseSendFunds {
    request_id: Uid,
    result: SendFundsResult,
}


// Indexer client to Networker
// ---------------------------

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseFriendsRoutes {
    pub routes: Vec<FriendsRouteWithCapacity>,
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
    
struct NeighborTokenChannelLoaded {
    channel_index: u32,
    local_max_debt: u64,
    remote_max_debt: u64,
    balance: i64,
}

struct NeighborLoaded {
    address: ChannelerAddress,
    max_channels: u32,
    wanted_remote_max_debt: u64,
    status: NeighborStatus,
    token_channels: Vec<NeighborTokenChannelLoaded>,
    // TODO: Should we use a map instead of a vector for token_channels?
}


enum NeighborTokenChannelEventInner {
    Open,
    Close,
    LocalMaxDebtChange(u64),    // Contains new local max debt
    RemoteMaxDebtChange(u64),   // Contains new remote max debt
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

pub enum NeighborStatus {
    Enabled,
    Disabled,
}

enum AppManagerToNetworker {
    RequestSendMessage(RequestSendMessage),
    ResponseMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
    SetNeighborWantedRemoteMaxDebt {
        neighbor_public_key: PublicKey,
        wanted_remote_max_debt: u64,
    },
    ResetNeighborChannel {
        neighbor_public_key: PublicKey,
        channel_index: u32,
        // TODO: Should we add wanted parameters for the ChannelReset, 
        // or let the Networker use the last Inconsistency message information 
        // to perform Reset?
    },
    SetNeighborMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
    AddNeighbor {
        neighbor_public_key: PublicKey,
        neighbor_address: ChannelerAddress,
        max_channels: u32,              // Maximum amount of token channels
        wanted_remote_max_debt: u64,    
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    SetNeighborStatus {
        neighbor_public_key: PublicKey,
        status: NeighborStatus,
    },
}


// Funder <--> Networker
// -------------------

enum FunderToNetworker {
    RequestSendMessage(RequestSendMessage),
    RespondMessageReceived(RespondMessageReceived),
    DiscardMessageReceived(DiscardMessageReceived),
    ResponseSendFunds(ResponseSendFunds),
}


enum NetworkerToFunder {
    MessageReceived(MessageReceived),
    ResponseSendMessage(ResponseSendMessage),
    RequestSendFunds(RequestSendFunds),
}


// Funder to Indexer Client
// ------------------------

pub struct FriendCapacity {
    send: u128,
    recv: u128,
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

pub enum IndexingProviderStatus {
    Enabled,
    Disabled,
}

pub struct IndexingProviderLoaded {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    status: IndexingProviderStatus,
}

pub enum AppManagerToIndexerClient {
    AddIndexingProvider(IndexingProviderInfo),
    SetIndexingProviderStatus {
        id: IndexingProviderId,
        status: IndexingProviderStatus,
    },
    RemoveIndexingProvider {
        id: IndexingProviderId,
    },
    RequestNeighborsRoutes(RequestNeighborsRoutes),
    RequestFriendsRoutes(RequestFriendsRoutes),
}

pub enum IndexingProviderStateUpdate {
    Loaded(IndexingProviderLoaded),
    ChainLinkUpdated(IndexingProviderInfo),
}

pub enum IndexerClientToAppManager {
    IndexingProviderStateUpdate(IndexingProviderStateUpdate),
    ResponseNeighborsRoutes(ResponseNeighborsRoutes),
    ResponseFriendsRoutes(ResponseFriendsRoutes),
}

pub enum IndexerClientToDatabase {
    StoreIndexingProvider(IndexingProviderInfo),
    RemoveIndexingProvider(IndexingProviderId),
    RequestLoadIndexingProviders,
    StoreRoute {
        id: IndexingProviderId,
        route: NeighborsRoute,
    },
}

pub struct IndexingProviderInfoFromDB {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    last_routes: Vec<NeighborsRoute>,
    status: IndexingProviderStatus,
}

pub enum DatabaseToIndexerClient {
    ResponseLoadIndexingProviders(Vec<IndexingProviderInfoFromDB>)
}

// Funder to App Manager
// ------------------------

// TODO: Not done here:
//
pub enum FriendStatus {
    Enabled,
    Disabled,
}

enum FriendRequestsStatus {
    Open,
    Close,
}

struct FriendLoaded {
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
    LocalMaxDebtChange(u128),   // Contains new local max debt
    RemoteMaxDebtChange(u128),   // Contains new local max debt
    BalanceChange(i128),        // Contains new balance
    InconsistencyError(i128),   // Contains balance required for reset
}

struct FriendStateUpdate {
    friend_public_key: PublicKey,
    event: FriendEvent,
}

enum FunderToAppManager {
    FriendStateUpdate(FriendStateUpdate),
    ResponseSendFunds(ResponseSendFunds),
}


// App Manager to Funder
// ------------------------

pub struct FriendInfo {
    friend_public_key: PublicKey,
    wanted_remote_max_debt: u128,
}


enum AppManagerToFunder {
    RequestSendFunds(RequestSendFunds),
    ResetFriendChannel {
        friend_public_key: PublicKey,
    },
    AddFriend {
        friend_info: FriendInfo,
    },
    RemoveFriend {
        friend_public_key: PublicKey,
    },
    SetFriendStatus {
        friend_public_key: PublicKey,
        status: FriendStatus,
        requests_status: FriendRequestsStatus,
    },
    SetFriendWantedRemoteMaxDebt {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
    },
}

// TODO: Before filling Funder <-> Database interface,
// check if we should merge the two Funder tables.
pub enum FunderToDatabase {
    StoreFriend {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
        status: FriendStatus,
        // TODO:
    },
    RemoveFriend {
        // TODO:
    },
    RequestLoadFriends {
        // TODO:
    },
    StoreInFriendToken {
        // TODO:
    },
    StoreOutFriendToken {
        // TODO:
    },
    RequestLoadFriendToken {
        // TODO:
    },
}

pub enum DatabaseToFunder {
    ResponseLoadFriends {
        // TODO:
    },
    ResponseLoadFriendToken {
        // TODO:
    },
}


pub struct NeighborMoveToken {
    token_channel_index: u32,
    transactions: Vec<Vec<u8>>, // TODO
    old_token: ChannelToken,
    rand_nonce: RandValue,
}

enum NeighborRequestType {
    CommMeans,
    Encrypted,
}


pub struct PendingNeighborRequest {
    request_id: Uid,
    route: NeighborsRoute,
    request_type: NeighborRequestType,
    request_content_hash: HashResult,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u32,
}


pub struct NeighborInfo {
    neighbor_public_key: PublicKey,
    neighbor_address: ChannelerAddress,
    wanted_remote_max_debt: u64,
    max_channels: u32,
    status: NeighborStatus,
}

pub enum NetworkerToDatabase {
    StoreNeighbor(NeighborInfo),    
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    RequestLoadNeighbors,
    StoreInNeighborToken {
        neighbor_public_key: PublicKey,
        move_token_message: NeighborMoveToken, 
        remote_max_debt: u64,
        local_max_debt: u64, 
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        closed_local_requests: Vec<Uid>,
        openend_remote_requests: Vec<PendingNeighborRequest>,
    },
    StoreOutNeighborToken {
        neighbor_public_key: PublicKey,
        move_token_message: NeighborMoveToken, 
        remote_max_debt: u64,
        local_max_debt: u64, 
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        opened_local_requests: Vec<PendingNeighborRequest>,
        closed_remote_requests: Vec<Uid>,
    },
    RequestLoadNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
    },
}

pub enum MoveTokenDirection {
    Incoming,
    Outgoing,
}

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors {
        neighbors: Vec<NeighborInfo>,
    },
    ResponseLoadNeighborToken {
        neighbor_public_key: PublicKey,
        move_token_direction: MoveTokenDirection,
        move_token_message: NeighborMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64, 
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        pending_local_requests: Vec<PendingNeighborRequest>,
        pending_remote_requests: Vec<PendingNeighborRequest>,
    },
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
