use std::collections::HashMap;
use std::net::SocketAddr;

use futures::sync::mpsc;

use crypto::identity::PublicKey;

use super::messages::ToChannel;

pub type NeighborsTable = HashMap<PublicKey, ChannelerNeighbor>;

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo {
    pub public_key:   PublicKey,
    pub socket_addr:  Option<SocketAddr>,
    pub max_channels: u32,
}

#[derive(Debug)]
pub struct ChannelerNeighbor {
    pub info:        ChannelerNeighborInfo,
    pub channels:    HashMap<u32, mpsc::Sender<ToChannel>>,
    pub retry_ticks: usize,
    pub num_pending: usize,
}
