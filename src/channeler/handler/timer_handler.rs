use std::rc::Rc;
use std::cell::RefCell;

use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use futures::future;
use futures::sync::mpsc;
use futures::{Async, Future, Stream, Poll};

use utils::CloseHandle;
use timer::messages::FromTimer;

use channeler::types::NeighborTable;
use channeler::channel::ChannelPool;
use channeler::handshake::{HandshakeClient, HandshakeServer};

pub fn create_timer_handler<SR: SecureRandom>(
    timer_receiver: mpsc::Receiver<FromTimer>,
    neighbors: Rc<RefCell<NeighborTable>>,
    channel_pool: Rc<RefCell<ChannelPool>>,
    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
) -> (impl Future<Item=(), Error=()>, CloseHandle) {
    let (close_handle, (remote_tx, remote_rx)) = CloseHandle::new();

    let core_task = timer_receiver.for_each(|FromTimer::TimeTick| {
        // TODO
        Ok(())
    });

    let handler = core_task.select(remote_rx.map_err(|_| {
        info!("closing because close handle dropped");
    })).then(|_| {
        match remote_tx.send(()) {
            Ok(()) => Ok(()),
            Err(_) => {
                error!("failed to send close event");
                Ok(())
            }
        }
    });

    (handler, close_handle)
}