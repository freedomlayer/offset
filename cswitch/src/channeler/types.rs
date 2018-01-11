use std::net::SocketAddr;

use futures::sync::mpsc;

use utils::crypto::uuid::Uuid;
use utils::crypto::identity::PublicKey;

use super::messages::ToChannel;

// TODO: Can the following two types be simplified?

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo {
    pub public_key: PublicKey,
    pub socket_addr: Option<SocketAddr>,
    pub max_channels: u32,
}

pub struct NeighborInfo {
    public_key: PublicKey,
    socket_addr: Option<SocketAddr>,
    max_channels: u32,
    token_channel_capacity: u64,
}

#[derive(Debug)]
pub struct ChannelerNeighbor {
    pub info: ChannelerNeighborInfo,
    pub channels: Vec<(Uuid, mpsc::Sender<ToChannel>)>,
    pub remaining_ticks: usize,
    pub num_pending_out_conn: usize,
}
