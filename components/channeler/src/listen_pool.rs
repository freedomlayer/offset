use std::collections::{HashMap, HashSet};
use futures::{Stream, StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};

use common::conn::{Listener, FutTransform};

use crypto::identity::PublicKey;
use crate::types::{RawConn, AccessControlPk, AccessControlOpPk};

pub enum LpConfig<B> {
    SetLocalAddress(Vec<B>),
    UpdateFriend((PublicKey, Vec<B>)),
    RemoveFriend(PublicKey),
}

#[derive(Debug)]
struct ListenPoolClientError;


pub struct LpConfigClient<B> {
    request_sender: mpsc::Sender<LpConfig<B>>,
}

impl<B> LpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<LpConfig<B>>) -> Self {
        LpConfigClient {
            request_sender,
        }
    }

    pub async fn config(&mut self, config: LpConfig<B>) -> Result<(), ListenPoolClientError> {
        await!(self.request_sender.send(config))
            .map_err(|_| ListenPoolClientError)?;
        Ok(())
    }
}

enum RelayStatus {
    Connecting,
    Connected,
}

struct Relay {
    friends: HashSet<PublicKey>,
    status: RelayStatus,
}

struct ListenPool<B> {
    local_address: Vec<B>,
    // friends: HashMap<PublicKey, Vec<B>>,
    relays: HashMap<B, Relay>,
}

#[derive(Debug)]
enum ListenPoolError {
}

enum ConnectPoolEvent {

}

async fn listen_pool_loop<B,L,TS,S>(incoming_config: mpsc::Receiver<LpConfig<B>>,
                             listener: L,
                             backoff_ticks: usize,
                             timer_stream: TS,
                             spawner: S) -> Result<(), ListenPoolError>
where
    L: Listener<Connection=(PublicKey, RawConn), 
        Config=AccessControlOpPk, Arg=(B, AccessControlPk)> + Clone + 'static,
    TS: Stream,
    S: Spawn,
{

    unimplemented!();
}

