use std::cmp::Ordering;
use std::collections::HashMap;

use futures::sync::mpsc;
use futures::{Future, Stream, Sink};

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker, ChannelClosed};
use security_module::security_module_client::SecurityModuleClient;
use futures_mutex::FutMutex;

use super::channel::Channel;
use super::{ChannelerNeighbor, ToChannel};

const CONN_ATTEMPT_TICKS: usize = 120;

#[derive(Debug)]
pub enum TimerReaderError {
    TimerReceiveFailed,
    SendToNetworkerFailed,
    FutMutex,
}

pub fn create_timer_reader_future(
    handle:           Handle,
    timer_receiver:   mpsc::Receiver<FromTimer>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    sm_client:        SecurityModuleClient,
    neighbors:        FutMutex<HashMap<PublicKey, ChannelerNeighbor>>
) -> impl Future<Item=(), Error=TimerReaderError> {
    timer_receiver.map_err(|()| TimerReaderError::TimerReceiveFailed)
        .map(move |FromTimer::TimeTick| {
            let handle           = handle.clone();
            let sm_client        = sm_client.clone();
            let neighbors        = neighbors.clone();
            let networker_sender = networker_sender.clone();

            (handle, networker_sender, sm_client, neighbors)
        })
        .for_each(move |(handle, networker_sender, sm_client, neighbors)| {
            let neighbors_inner1  = neighbors.clone();
            let neighbors_inner2  = neighbors.clone();
            // let networker_sender2 = networker_sender.clone();

            neighbors.clone().lock()
                .map_err(|_: ()| TimerReaderError::FutMutex)
                .and_then(move |mut neighbors| {
                    // Retry to establish connections when needed
                    let mut retry_conn_tasks = Vec::new();
                    for (neighbor_public_key, mut neighbor) in &mut neighbors.iter_mut() {
                        let addr = match neighbor.info.neighbor_address.socket_addr {
                            None => continue,
                            Some(addr) => addr,
                        };
                        match neighbor.channels.len().cmp(&(neighbor.info.max_channels as usize)) {
                            Ordering::Greater => unreachable!("number of channel exceeded the maximum"),
                            Ordering::Equal => continue,
                            Ordering::Less => {
                                if neighbor.channels.is_empty() {
                                    neighbor.remaining_retry_ticks -= 1;
                                    if neighbor.remaining_retry_ticks == 0 {
                                        neighbor.remaining_retry_ticks = CONN_ATTEMPT_TICKS;
                                    } else {
                                        continue;
                                    }
                                }
                            }
                        }

                        retry_conn_tasks.push((addr, neighbor_public_key.clone()));
                    }

                    for (addr, neighbor_public_key) in retry_conn_tasks {
                        let new_channel = Channel::connect(
                            &addr,
                            &handle,
                            &neighbor_public_key,
                            &neighbors_inner1,
                            &networker_sender,
                            &sm_client
                        ).map_err(|_| ());

                        handle.spawn(new_channel.then(|_| Ok(())));
                    }


                    // Notify all channels that time tick occurred
                    for (neighbor_public_key, mut neighbor) in &mut neighbors.iter() {
                        // let networker_sender = networker_sender.clone();
                        for &(ref channel_id, ref channel_sender) in &neighbor.channels {
                            let target_id           = channel_id.clone();
                            let neighbors           = neighbors_inner2.clone();
                            let neighbor_public_key = neighbor_public_key.clone();
                            let networker_sender    = networker_sender.clone();

                            let handle_inner   = handle.clone();
                            let handle_inner2  = handle.clone();
                            let send_tick_task = channel_sender.clone().send(ToChannel::TimeTick);

                            handle.spawn(send_tick_task.map_err(move |_e| {
                                debug!("channel dead, removing {:?}", target_id);

                                handle_inner.spawn(neighbors.clone().lock()
                                    .map_err(|_: ()| {
                                        error!("failed to acquire neighbors map");
                                    })
                                    .and_then(move |mut neighbors| {
                                        match neighbors.get_mut(&neighbor_public_key) {
                                            None => {
                                                error!("try to modify a nonexistent neighbor");
                                            }
                                            Some(neighbor) => {
                                                neighbor.channels.retain(|&(ref uid, _)| *uid != target_id);
                                                // Notify Networker if needed
                                                if neighbor.channels.is_empty() {
                                                    let msg = ChannelerToNetworker::ChannelClosed(
                                                        ChannelClosed {
                                                            remote_public_key: neighbor_public_key.clone(),
                                                        }
                                                    );
                                                    handle_inner2.spawn(
                                                        networker_sender.clone()
                                                            .send(msg)
                                                            .then(|_| Ok(()))
                                                    );
                                                }
                                            }
                                        }
                                        debug!("removing done");
                                        Ok(())
                                    })
                                );
                            }).then(|_| Ok(())));
                        }
                    }

                    Ok(())
                })
        })
}
