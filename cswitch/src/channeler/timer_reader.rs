use std::cmp::Ordering;
use std::collections::HashMap;

use futures::sync::mpsc;
use futures::future::join_all;
use futures::{Future, Stream, Sink, IntoFuture, Poll, Async};

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker};
use security_module::security_module_client::SecurityModuleClient;
use futures_mutex::FutMutex;

use super::channel::Channel;
use super::{ChannelerNeighbor, ToChannel};

const CONN_ATTEMPT_TICKS: usize = 120;

#[derive(Debug)]
pub enum TimerReaderError {
    TimerReceiveFailed,
    FutMutex,
}

//pub struct TimerReader {
//    timer_receiver: mpsc::Receiver<FromTimer>,
//    handle: Handle,
//    sm_client: SecurityModuleClient,
//    networker_sender: mpsc::Sender<ChannelerToNetworker>,
//    neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>,
//}
//
//impl Future for TimerReader {
//    type Item = ();
//    type Error = TimerReaderError;
//
//    fn poll(&mut self) -> Poll<(), TimerReaderError> {
//        match self.timer_receiver.poll() {
//            Err(_) => {
//                // TODO: Notify all component to close
//                error!("timer sender error, closing");
//                return Err(TimerReaderError::TimerReceiveFailed);
//            }
//            Ok(msg) => {
//                match msg {
//                    Async::NotReady => {
//                        return Ok(Async::NotReady);
//                    }
//                    Async::Ready(Some(FromTimer::TimeTick)) => {
//                        trace!("timer reader receive a tick");
//
//                        let handle = self.handle.clone();
//                        let sm_client = self.sm_client.clone();
//                        let neighbors_a = self.neighbors.clone();
//                        let networker
//
//                        self.neighbors.clone().lock()
//                            .map_err(|_| TimerReaderError::FutMutex)
//                            .and_then(|mut neighbors| {
//                                let mut attempt_tasks = Vec::new();
//                                for (neighbor_public_key, mut neighbor) in &mut neighbors.iter_mut() {
//                                    let addr = match neighbor.info.neighbor_address.socket_addr {
//                                        None => continue,
//                                        Some(addr) => addr,
//                                    };
//                                    match neighbor.channels.len().cmp(&(neighbor.info.max_channels as usize)) {
//                                        Ordering::Greater => unreachable!("number of channel exceeded the maximum"),
//                                        Ordering::Equal => continue,
//                                        Ordering::Less => {
//                                            if neighbor.channels.is_empty() {
//                                                neighbor.ticks_to_next_conn_attempt -= 1;
//                                                if neighbor.ticks_to_next_conn_attempt == 0 {
//                                                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
//                                                } else {
//                                                    continue;
//                                                }
//                                            }
//                                        }
//                                    }
//
//                                    attempt_tasks.push((addr, neighbor_public_key.clone()));
//                                }
//
//                                for (addr, neighbor_public_key) in attempt_tasks {
//                                    let new_channel = Channel::connect(&addr, &handle,
//                                                                       &neighbor_public_key,
//                                                                       &neighbors_a, &networker_tx,
//                                                                       &sm_client).map_err(|_| ());
//
//                                    handle.spawn(new_channel.then(|_| Ok(())));
//                                }
//                            })
//                    }
//                }
//            }
//        }
//    }
//}

pub fn create_timer_reader_future(handle: Handle,
                                  timer_receiver: mpsc::Receiver<FromTimer>,
                                  networker_sender: mpsc::Sender<ChannelerToNetworker>,
                                  security_module_client: SecurityModuleClient,
                                  neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>)
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
            neighbors.clone().lock()
                .map_err(|_: ()| TimerReaderError::FutMutex)
                .and_then(move |mut neighbors| {
                    // Attempt a new connection when needed
                    let mut attempt_tasks = Vec::new();
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
                    Ok(())
                })
                .and_then(move |mut neighbors| {
                    // Notify all channels that time tick occurred
                    // let mut notify_tasks = Vec::new();
                    for (neighbor_public_key, mut neighbor) in &mut neighbors.iter() {
                        let channels_len = neighbor.channels.len();
                        for &(ref channel_uid, ref channel_sender) in &neighbor.channels {
                            let channel_uid = channel_uid.clone();
                            let neighbor_public_key = neighbor_public_key.clone();
                            let neighbors = neighbors_b.clone();
                            let task = channel_sender.clone().send(ToChannel::TimeTick);
                            handle.spawn(task.map_err(move |e| {
                                // TODO： Remove this channel_sender
                                error!("channel dead, removing [{}] {:?}", channels_len, channel_uid);
                                neighbors.clone().lock()
                                    .map_err(|_: ()| {
                                        error!("Acquire Neighbors Failed!");
                                    })
                                    .and_then(move |mut tbl_neighbors_b| {
                                        match tbl_neighbors_b.get_mut(&neighbor_public_key) {
                                            None => error!("no such neighbor"),
                                            Some(neighbor) => {
                                                debug!("Get here!");
                                                neighbor.channels.retain(|&(ref uid, _)| *uid != channel_uid);
                                            }
                                        }
                                        debug!("remove: before return item");
                                        Ok(())
                                    })
                            }).then(|_| Ok(())));
                        }
                    }

                    Ok(())
                })
        })
}
