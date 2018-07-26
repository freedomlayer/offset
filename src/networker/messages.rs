#![allow(unused)]

use std::net::SocketAddr;

use bytes::Bytes;

use crypto::identity::PublicKey;
use crypto::uid::Uid;

use futures::sync::{mpsc, oneshot};

use channeler::types::ChannelerNeighborInfo;
use funder::messages::{RequestSendFunds};
use app_manager::messages::RequestFriendsRoute;

use database::messages::{ResponseLoadNeighbors, ResponseLoadNeighborToken};
use proto::networker::{ChannelToken, NetworkerSendPrice};

use super::messenger::token_channel::types::{TcBalance, TcInvoice, TcSendPrice};
use super::messenger::types::{NeighborsRoute, PendingNeighborRequest};



#[derive(Clone, Copy)]
pub enum NeighborStatus {
    Enable = 1,
    Disable = 0,
}


/// The neighbor's information from database.
pub struct NeighborInfo {
    pub neighbor_public_key: PublicKey,
    pub neighbor_socket_addr: Option<SocketAddr>,
    pub wanted_remote_max_debt: u64,
    pub wanted_max_channels: u16,
    pub status: NeighborStatus,
}

// ======== Internal Interfaces ========

pub struct NeighborUpdated {
    neighbor_socket_addr: Option<SocketAddr>,
    max_channels: u16,
    status: NeighborStatus,
}

enum RequestsStatus {
    Open(NetworkerSendPrice),
    Closed,
}

pub struct NeighborTokenChannelUpdated {
    channel_index: u16,
    balance: i64,
    local_max_debt: u64,
    remote_max_debt: u64,
    local_pending_debt: u64,
    remote_pending_debt: u64,
    requests_status: RequestsStatus,
}

pub struct NeighborTokenChannelInconsistent {
    channel_index: u16,
    current_token: ChannelToken,
    balance_for_reset: i64,
}

pub enum NeighborEvent {
    // NeighborLoaded(NeighborLoaded),
    // TokenChannelEvent(NeighborTokenChannelEvent),
    NeighborUpdated(NeighborUpdated),
    NeighborRemoved,
    NeighborTokenChannelUpdated(NeighborTokenChannelUpdated),
    NeighborTokenChannelInconsistent(NeighborTokenChannelInconsistent),
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

pub enum ResponsePath {
    Success(mpsc::Sender<RequestSendMessage>),
    // Proposal for opening the path was too low. This was discovered when the ResponseNonce
    // message was received.
    ProposalTooLow(u64), 
    // Returns the wanted amount of credits by remote side (Which was higher than what we were
    // willing to pay).
    Failure,
}

/// Component -> Networker
pub struct RequestPath {
    route: NeighborsRoute,
    path_fee_proposal: u64,
    response_sender: oneshot::Sender<ResponsePath>,
}

/// Component -> Networker
pub struct RequestSendMessage {
    // Note: We do not specify request_id here. 
    // It is the job of the Networker to randomly generate a random request_id internally.
    dest_port: DestinationPort,
    request_data: Vec<u8>,
    max_response_len: u32,
    processing_fee_proposal: u64,
    response_sender: oneshot::Sender<ResponseSendMessage>,
}

pub enum FailureSendMessage {
    // Could not reach remote using provided route. reportingPublicKey is supplied.
    Unreachable(PublicKey),
    // Message reached remote, but remote side lost the key (Possibly due to a reboot).
    RemoteLostKey,
    // We do not maintain such path with the given pathId locally.
    NoSuchPath,
}

/// Networker -> Component
pub enum ResponseSendMessage {
    Success(MessageReceivedResponse),
    Failure(FailureSendMessage), 
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
    ResponseSendMessage(ResponseSendMessage),
    MessageReceived(MessageReceived),
    RequestFriendsRoute(RequestFriendsRoute),
    NeighborStateUpdate(NeighborStateUpdate),
}

pub enum NetworkerToChanneler {
    /// Request send message to remote.
    SendChannelMessage {
        neighbor_public_key: PublicKey,
        content: Bytes,
    },
    /// Request to add a new neighbor.
    AddNeighbor {
        info: ChannelerNeighborInfo,
    },
    /// Request to remove a neighbor.
    RemoveNeighbor {
        neighbor_public_key: PublicKey
    },
}

pub struct NeighborTokenCommon {
    pub neighbor_public_key: PublicKey,
    pub move_token_message: Vec<u8>,
    // The move_token_message is opaque. The Database can not read it.
    // This is why we have the external token_channel_index, old_token and new_token,
    // although theoretically they could be deduced from move_token_message.
    pub token_channel_index: u16,
    pub old_token: ChannelToken,
    pub new_token: ChannelToken,
    // Equals Sha512/256(move_token_message)
    balance: TcBalance,
    invoice: TcInvoice,
    send_price: TcSendPrice,
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
    // TODO: StoreInNeighborToken should also create (atomically) relevant operations at the
    // pending table.
    StoreInNeighborToken(InNeighborToken),
    // TODO: StoreOutNeighborToken should also erase (atomically) the sent operations from the
    // pending table.
    StoreOutNeighborToken(OutNeighborToken),
    RequestLoadNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u16,
        response_sender: oneshot::Sender<Option<ResponseLoadNeighborToken>>,
    },
}

pub enum NetworkerToFunder {
    MessageReceived(MessageReceived),
    RequestSendFunds(RequestSendFunds),
}

/*
pub enum NetworkerToIndexerClient {
    MessageReceived(MessageReceived),
    RequestFriendsRoutes(RequestFriendsRoutes),
}
*/
