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

pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;
pub const RECEIPT_RESPONSE_HASH_LEN: usize = 32;
pub const INDEXING_PROVIDER_ID_LEN: usize = 16;
pub const INVOICE_ID_LEN: usize = 32;

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
struct InvoiceId([u8; INVOICE_ID_LEN]);

/// A hash of:
/// = sha512/256(requestId || 
///       sha512/256(nodeIdPath) || 
///       mediatorPaymentProposal)
/// Used inside SendFundsReceipt
struct ReceiptResponseHash([u8; RECEIPT_RESPONSE_HASH_LEN]);

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
    wanted_remote_max_debt: u64,    
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

enum NeighborStatus {
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
        neighbor_info: NeighborInfo,
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


pub enum FunderToIndexerClient {
    RequestNeighborsRoute(RequestNeighborsRoutes),
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
enum FriendStatus {
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

pub enum FunderToDatabase {
    StoreFriend {
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

//<<<<<<< HEAD
//pub struct StoreNeighborInfo {
//    pub neighbor_public_key: PublicKey,
//    pub remote_maximum_debt: u64,
//    pub maximum_channel: u32,
//    pub is_enable: bool
//}
//
//pub struct MoveTokenMessage {
//    pub move_token_transactions: Bytes,
//    // pub move_token_old_token: TODO
//    pub move_tolen_rand_nonce: RandValue,
//=======
//pub enum NetworkerToDatabase {
//    StoreNeighbor {
//        // TODO
//    },
//    RemoveNeighbor {
//        // TODO
//    },
//    RequestLoadNeighbors {
//        // TODO
//    },
//    StoreInNeighborToken {
//        // TODO
//    },
//    StoreOutNeighborToken {
//        // TODO
//    },
//    RequestLoadNeighborToken {
//        // TODO
//    },
//}
//
//pub enum DatabaseToNetworker {
//    ResponseLoadNeighbors {
//        // TODO
//    },
//    ResponseLoadNeighborToken {
//        // TODO
//    },
//}

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
