use std::collections::HashMap;
use futures::sync::mpsc;
use crypto::identity::PublicKey;
use super::messages::ToChannel;

pub type NeighborsTable<A> = HashMap<PublicKey, ChannelerNeighbor<A>>;

#[derive(Clone, Debug)]
pub struct ChannelerNeighborInfo<A> {
    pub public_key: PublicKey,
    pub address: Option<A>,
}

#[derive(Debug)]
pub struct ChannelerNeighbor<A> {
    pub info: ChannelerNeighborInfo<A>,
    pub channel: Option<mpsc::Sender<ToChannel>>,
    pub retry_ticks: usize,
    pub num_pending: usize,
}
