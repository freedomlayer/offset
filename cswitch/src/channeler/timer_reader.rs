extern crate tokio_core;

use std::collections::HashMap;

use futures::{Future, Stream, Sink};
use futures::sync::mpsc;
use futures::future::join_all;
use self::tokio_core::reactor::Handle;

use std::cell::RefCell;
use std::rc::Rc;

use ::inner_messages::{FromTimer, ChannelerToNetworker};
use ::security_module::security_module_client::SecurityModuleClient;
use ::crypto::rand_values::RandValuesStore; 
use ::crypto::identity::{PublicKey};
use super::{ChannelerNeighbor, ToChannel};
use super::channel::create_channel;


const CONN_ATTEMPT_TICKS: usize = 120;

pub enum TimerReaderError {
    TimerReceiveFailed,
}

// TODO: Possibly change Handle to &Handle? Will it compile?
pub fn timer_reader_future<R>(handle: Handle,
                           timer_receiver: mpsc::Receiver<FromTimer>,
                           networker_sender: mpsc::Sender<ChannelerToNetworker>, 
                           security_module_client: SecurityModuleClient,
                           crypt_rng: Rc<R>,
                           rand_values_store: Rc<RefCell<RandValuesStore>>,
                           neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>)
                -> impl Future<Item=(), Error=TimerReaderError> {

    timer_receiver
    .map_err(|()| TimerReaderError::TimerReceiveFailed)
    .for_each(move |FromTimer::TimeTick| {
        // Attempt new connections whenever needed:
        let mut neighbors = (*neighbors).borrow_mut();
        for (_, mut neighbor) in &mut *neighbors {
            let socket_addr = match neighbor.info.neighbor_address.socket_addr {
                None => continue,
                Some(socket_addr) => socket_addr,
            };

            if neighbor.channel_senders.len() > (neighbor.info.max_channels as usize) {
                unreachable!();
            }

            if neighbor.channel_senders.len() == (neighbor.info.max_channels as usize){
                continue;
            }

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

            let channel = create_channel(&handle, 
                                socket_addr, &neighbor.info.neighbor_address.neighbor_public_key);
            handle.spawn(channel.map_err(|_| ()));
            // TODO: This number should be decreased when we get an actual TCP connection.
            neighbor.num_pending_out_conn += 1;
        }

        // Report to all current connections that TimeTick has occured:
        // TODO: Should this be done on the same task, or spawned to a new task?
        let mut send_futures = Vec::new();
        for (_, neighbor) in &*neighbors {
            for channel_sender in &neighbor.channel_senders {
                let send_timetick = channel_sender
                    .clone()
                    .send(ToChannel::TimeTick)
                    .map_err(|_e: mpsc::SendError<ToChannel>| ());
                send_futures.push(send_timetick);
            }
        }
        handle.spawn(join_all(send_futures).then(|_| Ok(())));

        Ok(())
    })
}
