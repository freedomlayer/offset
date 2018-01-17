use std::net::SocketAddr;

use bytes::Bytes;

use crypto::identity::PublicKey;
use crypto::hash::HashResult;
use crypto::uuid::Uuid;

use channeler::types::ChannelerNeighborInfo;

use proto::indexer::NeighborsRoute;
use proto::funder::InvoiceId;
use proto::networker::{NeighborRequestType, NeighborMoveToken};

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

pub struct PendingNeighborRequest {
    pub request_id: Uuid,
    pub route: NeighborsRoute,
    pub request_type: NeighborRequestType,
    pub request_content_hash: HashResult,
    pub max_response_length: u32,
    pub processing_fee_proposal: u64,
    pub credits_per_byte_proposal: u32,
}

/// The neighbor's information from database.
pub struct NeighborInfo {
    pub neighbor_public_key: PublicKey,
    pub neighbor_socket_addr: Option<SocketAddr>,
    pub wanted_remote_max_debt: u64,
    pub wanted_max_channels: u32,
    pub status: NeighborStatus,
}

// ========== Interface with the Channeler ==========

/// The internal message sent from `Networker` to `Channeler`.
pub enum NetworkerToChanneler {
    /// Request to send a message via given `Channel`.
    SendChannelMessage {
        neighbor_public_key: PublicKey,
        channel_index: u32,
        content: Bytes,
    },
    /// Request to add a new neighbor.
    AddNeighbor {
        neighbor_info: ChannelerNeighborInfo,
    },
    /// Request to delete a neighbor.
    RemoveNeighbor { neighbor_public_key: PublicKey },
    /// Request to set the maximum amount of token channel.
    SetMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
}

// ========== Interface with the Database ==========

pub enum NetworkerToDatabase {
    StoreNeighbor(NeighborInfo),
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    RequestLoadNeighbors,
    StoreInNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
        move_token_message: NeighborMoveToken,
        remote_max_debt: u64,
        local_max_debt: u64,
        remote_pending_debt: u64,
        local_pending_debt: u64,
        balance: i64,
        local_invoice_id: Option<InvoiceId>,
        remote_invoice_id: Option<InvoiceId>,
        closed_local_requests: Vec<Uuid>,
        opened_remote_requests: Vec<PendingNeighborRequest>,
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
        closed_remote_requests: Vec<Uuid>,
    },
    RequestLoadNeighborToken {
        neighbor_public_key: PublicKey,
        token_channel_index: u32,
    },
}

pub enum DatabaseToNetworker {
    ResponseLoadNeighbors {
        neighbors: Vec<NeighborInfo>
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