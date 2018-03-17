use std::collections::HashMap;
use std::net::SocketAddr;

use crypto::identity::PublicKey;

pub type NeighborsTable = HashMap<PublicKey, ChannelerNeighbor>;

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo {
    pub public_key: PublicKey,
    pub socket_addr: Option<SocketAddr>,
}

#[derive(Debug)]
pub struct ChannelerNeighbor {
    pub info: ChannelerNeighborInfo,
    pub retry_ticks: usize,
}