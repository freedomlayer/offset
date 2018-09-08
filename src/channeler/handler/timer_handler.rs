use std::rc::Rc;
use std::cell::RefCell;
use std::net::SocketAddr;

use bytes::Bytes;
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use futures::future;
use futures::prelude::*;
use futures::sync::mpsc;
use futures::{Async, Future, Stream, Poll};

use utils::CloseHandle;
use timer::messages::FromTimer;

use channeler::types::NeighborTable;
use channeler::channel::ChannelPool;
use channeler::handshake::{HandshakeClient, HandshakeServer};

use proto::Proto;
use proto::channeler::ChannelerMessage;

pub fn create_timer_handler<SR, O, E>(
    timer_receiver: mpsc::Receiver<FromTimer>,
    external_sender: O,
    neighbors: Rc<RefCell<NeighborTable>>,
    channel_pool: Rc<RefCell<ChannelPool>>,
    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
) -> (impl Future<Item=(), Error=()>, CloseHandle)
    where
        SR: SecureRandom,
        O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=E>,
        E: ::std::error::Error,
{
    let (close_handle, (remote_tx, remote_rx)) = CloseHandle::new();

    let core_task = timer_receiver.for_each(move |FromTimer::TimeTick| {
        for neighbor in neighbors.borrow_mut().values_mut() {
            // If current neighbor have disconnected and no relevant handshake
            if channel_pool.borrow().is_connected(neighbor.remote_public_key())
                || handshake_client.borrow().allow_initiate_handshake(neighbor.remote_public_key()).is_err() {
                continue;
            }
            if neighbor.reconnect_timeout <= 1 {
                // TODO: Try to initiate a new handshake, reset counter ONLY when send initiate
                // packet successful. If send fail, reconnect will retry next time tick.
//                let request_nonce_msg = handshake_client.borrow_mut()
//                    .initiate_handshake(neighbor.remote_public_key().clone())
//                    .map_err(ChannelerError::HandshakeError)
//                    .and_then(move |request_nonce| {
//                        ChannelerMessage::RequestNonce(request_nonce)
//                            .encode()
//                            .map_err(ChannelerError::ProtoError)
//                    });
//                let _ = external_sender.try_send();
            } else {
                neighbor.reconnect_timeout -= 1;
            }

            handshake_client.borrow_mut().time_tick();
            handshake_server.borrow_mut().time_tick();


            // TODO: Check established channel keepalive timeout
        }
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