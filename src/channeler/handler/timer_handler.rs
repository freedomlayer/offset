use std::rc::Rc;
use std::cell::RefCell;

use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use futures::sync::mpsc;
use futures::{Async, Future, Stream, Poll};

use utils::CloseHandle;
use timer::messages::FromTimer;

use channeler::types::NeighborTable;
use channeler::channel::ChannelPool;
use channeler::handshake::{HandshakeClient, HandshakeServer};

pub struct TimerHandler<SR> {
    timer_receiver: mpsc::Receiver<FromTimer>,

    // Shared resources between handlers.
    neighbors: Rc<RefCell<NeighborTable>>,
    channel_pool: Rc<RefCell<ChannelPool>>,
    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
}

impl<SR: SecureRandom> TimerHandler<SR> {
    pub fn new(
        timer_receiver: mpsc::Receiver<FromTimer>,
        neighbors: Rc<RefCell<NeighborTable>>,
        channel_pool: Rc<RefCell<ChannelPool>>,
        handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
        handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
    ) -> TimerHandler<SR> {
        unimplemented!() 
    }

    pub fn run(executor: Handle) -> CloseHandle {
        unimplemented!()
    }
}