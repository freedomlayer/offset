use std::rc::Rc;
use std::cell::RefCell;

use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use futures::sync::mpsc;
use futures::{Async, Future, Stream, Poll};

use utils::CloseHandle;
use networker::messages::NetworkerToChanneler;

use channeler::types::NeighborTable;
use channeler::channel::ChannelPool;
use channeler::handshake::{HandshakeClient, HandshakeServer};

pub struct NetworkerHandler<SR> {
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    // external_sender

    // Shared resources between handlers.
    neighbors: Rc<RefCell<NeighborTable>>,
    channel_pool: Rc<RefCell<ChannelPool>>,
    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
}

impl<SR: SecureRandom> NetworkerHandler<SR> {
    pub fn new(
        networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
        neighbors: Rc<RefCell<NeighborTable>>,
        channel_pool: Rc<RefCell<ChannelPool>>,
        handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
        handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
    ) -> NetworkerHandler<SR> {
        unimplemented!() 
    }

    pub fn run(executor: Handle) -> CloseHandle {
        unimplemented!()
    }
}