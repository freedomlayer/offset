use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use futures::sync::mpsc;
use futures::future::join_all;
use futures::{Future, Stream, Sink};

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker};
use security_module::security_module_client::SecurityModuleClient;

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
                    unreachable!("The number of channel exceeded the max channels");
                }

                if neighbor.channel_senders.len() == (neighbor.info.max_channels as usize) {
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
                                             &socket_addr,
                                             &neighbor.info.neighbor_address.neighbor_public_key,
                                             &security_module_client);
                handle.spawn(channel.map_err(|_| ()));

                // TODO: This number should be decreased when we get an actual TCP connection.
                // TODO: We need to notify the Networker if it is the only connection.
                neighbor.num_pending_out_conn += 1;
            }

            // Notify all current connections that TimeTick has occurred
            let send_time_tick_tasks = neighbors.iter()
                .map(|(_, neighbor)| {
                    neighbor.channel_senders.iter()
                        .map(|channel_sender| {
                            channel_sender
                                .clone()
                                .send(ToChannel::TimeTick)
                                .map_err(|_e: mpsc::SendError<ToChannel>| ())
                        })
                }).flat_map(|task| task).collect::<Vec<_>>();

            // TODO: Should this be done on the same task, or spawned to a new task?
            // TODO: If failed to send Time Tick, would this Channel alive？
            handle.spawn(join_all(send_time_tick_tasks).then(|_| Ok(())));

            Ok(())
        })
}
