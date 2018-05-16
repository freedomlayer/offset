use std::net::SocketAddr;

use bytes::Bytes;

use crypto::identity::PublicKey;
use crypto::hash::HashResult;
use crypto::uid::Uid;

use futures::sync::{mpsc, oneshot};

use channeler::types::ChannelerNeighborInfo;
use funder::messages::{RequestSendFunds};

use indexer_client::messages::RequestFriendsRoutes;
use database::messages::{ResponseLoadNeighbors, ResponseLoadNeighborToken};
use proto::indexer::NeighborsRoute;
use proto::funder::InvoiceId;
use proto::networker::ChannelToken;

use networker::messenger::credit_calculator;
use utils::convert_int;


use super::messenger::pending_neighbor_request::PendingNeighborRequest;

/// Indicate the direction of the move token message.
#[derive(Clone, Copy, Debug)]
pub enum MoveTokenDirection {
    Incoming = 0,
    Outgoing = 1,
}

pub enum NeighborStatus {
    Enable = 1,
    Disable = 0,
}


/// The neighbor's information from database.
pub struct NeighborInfo {
    pub neighbor_public_key: PublicKey,
    pub neighbor_socket_addr: Option<SocketAddr>,
    pub wanted_remote_max_debt: u64,
    pub wanted_max_channels: u32,
    pub status: NeighborStatus,
}

// ======== Internal Interfaces ========

struct NeighborTokenChannelLoaded {
    channel_index: u32,
    local_max_debt: u64,
    remote_max_debt: u64,
    balance: i64,
}

pub enum NeighborTokenChannelEventInner {
    Open,
    Close,
    LocalMaxDebtChange(u64),  // Contains new local max debt
    RemoteMaxDebtChange(u64), // Contains new remote max debt
    BalanceChange(i64),       // Contains new balance
    InconsistencyError(i64),  // Contains balance required for reset
}

pub struct NeighborLoaded {
    neighbor_public_key: PublicKey,
    neighbor_socket_addr: Option<SocketAddr>,
    max_channels: u32,
    wanted_remote_max_debt: u64,
    status: NeighborStatus,
    token_channels: Vec<NeighborTokenChannelLoaded>,
    // TODO: Should we use a map instead of a vector for token_channels?
}

pub struct NeighborTokenChannelEvent {
    channel_index: u32,
    event: NeighborTokenChannelEventInner,
}

pub enum NeighborEvent {
    NeighborLoaded(NeighborLoaded),
    TokenChannelEvent(NeighborTokenChannelEvent),
}

pub struct NeighborStateUpdate {
    neighbor_public_key: PublicKey,
    event: NeighborEvent,
}


/// Destination port for the packet.
/// The destination port is used by the destination Networker to know where to forward the received
/// message.
pub enum DestinationPort {
    Funder,
    IndexerClient,
    AppManager(u32),
}

/// Component -> Networker
pub struct RequestPath {
    route: NeighborsRoute,
    response_sender: oneshot::Sender<Option<mpsc::Sender<RequestSendMessage>>>,
}

/// Component -> Networker
pub struct RequestSendMessage {
    // route: NeighborsRoute,
    dest_port: DestinationPort,
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u32,
    response_sender: oneshot::Sender<ResponseSendMessage>,
}

/// Networker -> Component
pub enum ResponseSendMessage {
    Success(Vec<u8>),
    Failure,
}

/// Networker -> Component
pub struct MessageReceived {
    route: NeighborsRoute, // sender_public_key is the first public key on the NeighborsRoute
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    credits_per_byte_proposal: u32,
    response_sender: oneshot::Sender<MessageReceivedResponse>,
}


/// Component -> Networker
pub struct MessageReceivedResponse {
    response_data: Vec<u8>,
    processing_fee_collected: u64,
}


pub enum NetworkerToAppManager {
    MessageReceived(MessageReceived),
    ResponseSendMessage(ResponseSendMessage),
    NeighborStateUpdate(NeighborStateUpdate),
}

pub enum NetworkerToChanneler {
    /// Request to add a new neighbor.
    AddNeighbor {
        info: ChannelerNeighborInfo,
        /// Possibly return a sender. This sender is then used to send
        /// messages to the new neighbor.
        /// Communication to the new neighbor is closed by dropping the sender.
        response_sender: oneshot::Sender<Option<mpsc::Sender<Vec<u8>>>>,
    },
}

pub struct NeighborTokenCommon {
    pub neighbor_public_key: PublicKey,
    pub token_channel_index: u32,
    pub move_token_message: Vec<u8>,
    // The move_token_message is opaque. The Database can not read it.
    // This is why we have the external token_channel_index, old_token and new_token,
    // although theoretically they could be deduced from move_token_message.
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(move_token_message)
    pub remote_max_debt: u64,
    pub local_max_debt: u64,
    pub remote_pending_debt: u64,
    pub local_pending_debt: u64,
    pub balance: i64,
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
}

pub struct InNeighborToken {
    pub neighbor_token_common: NeighborTokenCommon,
    pub closed_local_requests: Vec<Uid>,
    pub opened_remote_requests: Vec<PendingNeighborRequest>,
}

pub struct OutNeighborToken {
    pub neighbor_token_common: NeighborTokenCommon,
    pub opened_local_requests: Vec<PendingNeighborRequest>,
    pub closed_remote_requests: Vec<Uid>,
}

#[allow(large_enum_variant)]
pub enum NetworkerToDatabase {
    StoreNeighbor(NeighborInfo),
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    RequestLoadNeighbors {
        response_sender: oneshot::Sender<ResponseLoadNeighbors>,
    },
    StoreInNeighborToken(InNeighborToken),
    StoreOutNeighborToken(OutNeighborToken),
    RequestLoadNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
        response_sender: oneshot::Sender<Option<ResponseLoadNeighborToken>>,
    },
}

pub enum NetworkerToFunder {
    MessageReceived(MessageReceived),
    RequestSendFunds(RequestSendFunds),
}

pub enum NetworkerToIndexerClient {
    MessageReceived(MessageReceived),
    RequestFriendsRoutes(RequestFriendsRoutes),
}
