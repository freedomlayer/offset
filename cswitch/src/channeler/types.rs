use std::collections::HashMap;
use std::net::SocketAddr;

use futures::sync::mpsc;

use utils::crypto::identity::PublicKey;

use super::messages::ToChannel;

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
    pub channels: HashMap<u32, mpsc::Sender<ToChannel>>,
    pub retry_ticks: usize,
    pub num_pending: usize,
}
