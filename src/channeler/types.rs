use std::collections::HashMap;
use std::net::SocketAddr;

use crypto::identity::PublicKey;

pub type NeighborTable = HashMap<PublicKey, ChannelerNeighbor>;

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo {
    pub public_key: PublicKey,
    pub socket_addr: Option<SocketAddr>,
}

#[derive(Debug)]
pub struct ChannelerNeighbor {
    info: ChannelerNeighborInfo,
    pub reconnect_timeout: usize,
}

impl ChannelerNeighbor {
    pub fn new(info: ChannelerNeighborInfo) -> ChannelerNeighbor {
        ChannelerNeighbor { info, reconnect_timeout: 0 }
    }

    #[inline]
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.info.socket_addr
    }

    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        &self.info.public_key
    }
}