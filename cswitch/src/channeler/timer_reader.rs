extern crate futures;
extern crate tokio_core;

use std::collections::HashMap;

use self::futures::{Future, Stream};
use self::futures::sync::mpsc;
use self::tokio_core::reactor::Handle;

use std::cell::RefCell;
use std::rc::Rc;

use ::async_mutex::AsyncMutex;
use ::inner_messages::{FromTimer, ChannelerToNetworker};
use ::security_module::security_module_client::SecurityModuleClient;
use ::crypto::rand_values::RandValuesStore; 
use ::crypto::identity::{PublicKey};
use super::ChannelerNeighbor;
use super::channel::create_channel;


const CONN_ATTEMPT_TICKS: usize = 120;

pub enum TimerReaderError {
    TimerReceiveFailed,
}

// TODO: Possibly change Handle to &Handle? Will it compile?
pub fn timer_reader_future<R>(handle: Handle,
                           timer_receiver: mpsc::Receiver<FromTimer>,
                           am_networker_sender: AsyncMutex<mpsc::Sender<ChannelerToNetworker>>, 
                           security_module_client: SecurityModuleClient,
                           crypt_rng: Rc<R>,
                           rand_values_store: Rc<RefCell<RandValuesStore>>,
                           neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>)
                -> impl Future<Item=(), Error=TimerReaderError> {

    timer_receiver
    .map_err(|()| TimerReaderError::TimerReceiveFailed)
    .for_each(move |FromTimer::TimeTick| {
        // TODO:
        // - Attempt new connections whenever needed.
        // - Report all connections that time has passed (Sending through a one directional
        //      mpsc::channel?)
        
        let ref mut neighbors = *(*neighbors).borrow_mut();
        for (_, mut neighbor) in neighbors {
            let socket_addr = match neighbor.info.neighbor_address.socket_addr {
                None => continue,
                Some(socket_addr) => socket_addr,
            };
            // If there are already some attempts to add connections, 
            // we don't try to add a new connection ourselves.
            if neighbor.num_pending_out_conn > 0 {
                continue;
            }
            if neighbor.channel_senders.len() == 0 {
                // This is an inactive neighbor.
                neighbor.ticks_to_next_conn_attempt -= 1;
                if neighbor.ticks_to_next_conn_attempt == 0 {
                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
                } else {
                    continue;
                }
            }

            // create_channel(handle: &Handle, socket_addr: SocketAddr ,neighbor_public_key: &PublicKey) 

            let (channel_sender, channel) = create_channel(&handle, socket_addr, &neighbor.info.neighbor_address.neighbor_public_key);
            neighbor.channel_senders.push(channel_sender);

            handle.spawn(channel.map_err(|_| ()));
        }
        Ok(())
    })
}
