use std::collections::HashMap;
use futures::channel::mpsc;
use crypto::identity::PublicKey;

pub enum LpConfig<B> {
    SetLocalAddress(Vec<B>),
    UpdateFriend((PublicKey, Vec<B>)),
    RemoveFriend(PublicKey),
}


pub struct LpConfigClient<B> {
    request_sender: mpsc::Sender<LpConfig<B>>,
}

impl<B> LpConfigClient<B> {
    pub fn new(request_sender: mpsc::Sender<LpConfig<B>>) -> Self {
        LpConfigClient {
            request_sender,
        }
    }
}

struct ListenPool<B> {
    local_address: Vec<B>,
    friends: HashMap<PublicKey, Vec<B>>,
}

async fn listen_pool_loop<B>(incoming_config: mpsc::Receiver<LpConfig<B>>) {
}

