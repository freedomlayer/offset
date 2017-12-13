use std::cmp::Ordering;
use std::collections::HashMap;

use futures::sync::mpsc;
use futures::future::join_all;
use futures::{Future, Stream, Sink, IntoFuture};

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker};
use security_module::security_module_client::SecurityModuleClient;
use async_mutex::{AsyncMutex, AsyncMutexError};

use super::channel::Channel;
use super::{ChannelerNeighbor, ToChannel};

const CONN_ATTEMPT_TICKS: usize = 120;

pub enum TimerReaderError {
    TimerReceiveFailed,
    AsyncMutexError,
}

pub fn create_timer_reader_future(handle: Handle,
                                  timer_receiver: mpsc::Receiver<FromTimer>,
                                  networker_sender: mpsc::Sender<ChannelerToNetworker>,
                                  security_module_client: SecurityModuleClient,
                                  neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>)
    -> impl Future<Item=(), Error=TimerReaderError> {
    timer_receiver.map_err(|()| TimerReaderError::TimerReceiveFailed)
        .map(move |FromTimer::TimeTick| {
            let new_handle = handle.clone();
            let networker_tx = networker_sender.clone();
            let sm_client = security_module_client.clone();
            let tbl_neighbors = neighbors.clone();

            (new_handle, networker_tx, sm_client, tbl_neighbors)
        })
        .for_each(move |(handle, networker_tx, sm_client, neighbors)| {
            trace!("timer reader receive a tick");
            let neighbors_a = neighbors.clone();
            let neighbors_b = neighbors.clone();
            neighbors.acquire(move |mut tbl_neighbors| {
                // Attempt a new connection when needed
                let mut attempt_tasks = Vec::new();
                for (neighbor_public_key, mut neighbor) in &mut tbl_neighbors {
                    let addr = match neighbor.info.neighbor_address.socket_addr {
                        None => continue,
                        Some(addr) => addr,
                    };
                    match neighbor.channels.len().cmp(&(neighbor.info.max_channels as usize)) {
                        Ordering::Greater => unreachable!("number of channel exceeded the maximum"),
                        Ordering::Equal => continue,
                        Ordering::Less => {
                            if neighbor.channels.is_empty() {
                                neighbor.ticks_to_next_conn_attempt -= 1;
                                if neighbor.ticks_to_next_conn_attempt == 0 {
                                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
                                } else {
                                    continue;
                                }
                            }
                        }
                    }

                    attempt_tasks.push((addr, neighbor_public_key.clone()));
                }

                for (addr, neighbor_public_key) in attempt_tasks {
                    let new_channel = Channel::connect(&addr, &handle,
                                                       &neighbor_public_key,
                                                       &neighbors_a, &networker_tx,
                                                       &sm_client).map_err(|_| ());

                    handle.spawn(new_channel.then(|_| Ok(())));
                }

                // Notify all channels that time tick occurred
                // let mut notify_tasks = Vec::new();

                for (neighbor_public_key, mut neighbor) in &mut tbl_neighbors {
                    let channels_len = neighbor.channels.len();
                    for &(ref channel_uid, ref channel_sender) in &neighbor.channels {
                        let channel_uid = channel_uid.clone();
                        let neighbor_public_key = neighbor_public_key.clone();
                        let neighbors = neighbors_b.clone();
                        let task = channel_sender.clone()
                            .send(ToChannel::TimeTick)
                            .map_err(move |_e: mpsc::SendError<ToChannel>| {
                                // TODO： Remove this channel_sender
                                error!("channel dead, removing [{}] {:?}", channels_len, channel_uid);
//                                neighbors.acquire(move |mut tbl_neighbors_b| {
//                                    match tbl_neighbors_b.get_mut(&neighbor_public_key) {
//                                        None => error!("no such neighbor"),
//                                        Some(neighbor) => {
//                                            neighbor.channels.retain(|&(ref uid, _)| *uid != channel_uid);
//                                        }
//                                    }
//                                    debug!("remove: before return item");
//                                    Ok((tbl_neighbors_b, ()))
//                                }).map_err(|_: AsyncMutexError<()>| ())
                            });
                        // notify_tasks.push(task);
                        handle.spawn(task.then(|_| Ok(())));
                    }
                }

                // handle.spawn(join_all(notify_tasks).then(|_| Ok(())));

                trace!("timer reader: before return item");
                Ok((tbl_neighbors, ()))
            }).map_err(|_: AsyncMutexError<()>| TimerReaderError::AsyncMutexError)
        })
}
