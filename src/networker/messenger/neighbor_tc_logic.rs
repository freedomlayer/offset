use std::collections::HashMap;

use proto::networker::{NeighborMoveToken, ChannelToken};
use proto::funder::InvoiceId;
use crypto::uid::Uid;
use super::NeighborTokenChannel;
use super::super::messages::{MoveTokenDirection, PendingNeighborRequest};

struct NeighborTCState {
    pub move_token_direction: MoveTokenDirection,
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
    pub pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pub pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

fn receive_move_token(neighbor_tc: &mut NeighborTCState, move_token_message: &NeighborMoveToken, new_token: ChannelToken) {
}

fn send_move_token(neighbor_tc: &mut NeighborTCState, move_token_message: &NeighborMoveToken, new_token: ChannelToken) {
}
