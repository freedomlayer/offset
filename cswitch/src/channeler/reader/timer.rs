use std::mem;
use std::cmp::Ordering;
use std::collections::HashMap;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};


use tokio_core::reactor::Handle;

use async_mutex::{AsyncMutex, AsyncMutexError};
use crypto::identity::PublicKey;
//use inner_messages::{FromTimer, ChannelerToNetworker, ChannelClosed, ChannelOpened};
use close_handle::{CloseHandle, create_close_handle};
use security_module::security_module_client::SecurityModuleClient;

use channeler::types::ChannelerNeighbor;
use channeler::messages::{ToChannel, ChannelerToNetworker, ChannelEvent};

const CONN_ATTEMPT_TICKS: usize = 120;

#[derive(Debug)]
pub enum TimerReaderError {
    TimerReceiveFailed,
    RemoteCloseHandleClosed,
    SendToNetworkerFailed,
    FutMutex,
}

impl From<oneshot::Canceled> for TimerReaderError {
    #[inline]
    fn from(_e: oneshot::Canceled) -> TimerReaderError {
        TimerReaderError::RemoteCloseHandleClosed
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct TimerReader {
    handle: Handle,
    neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    sm_client: SecurityModuleClient,

    inner_tx: mpsc::Sender<ChannelerToNetworker>,
    inner_rx: mpsc::Receiver<FromTimer>,

    // For reporting the closing event
    close_tx:   Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl TimerReader {
    pub fn new(
        handle:           Handle,
        timer_receiver:   mpsc::Receiver<FromTimer>,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client:        SecurityModuleClient,
        neighbors:        AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>
    ) -> (CloseHandle, TimerReader) {
        let (close_handle, (close_tx, close_rx)) = create_close_handle();

        let timer_reader = TimerReader {
            handle,
            sm_client,
            neighbors,
            close_tx: Some(close_tx),
            close_rx: close_receiver,
            inner_tx: networker_sender,
            inner_rx: timer_receiver,
        };

        (close_handle, timer_reader)
    }

    #[inline]
    fn retry_conn(&self) {
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = self.neighbors.clone();
        let sm_client_for_task = self.sm_client.clone();
        let networker_sender_for_task = self.inner_tx.clone();

        let retry_conn_task = self.neighbors.clone().lock()
            .map_err(|_: ()| TimerReaderError::FutMutex)
            .and_then(move |mut neighbors| {
                let mut retry_conn_tasks = Vec::new();

                for (neighbor_public_key, mut neighbor) in &mut neighbors.iter_mut() {
                    let addr = match neighbor.info.neighbor_address.socket_addr {
                        None => continue,
                        Some(addr) => addr,
                    };

                    if neighbor.num_pending_out_conn > 0 {
                        continue;
                    }

                    match neighbor.channels.len().cmp(&(neighbor.info.max_channels as usize)) {
                        Ordering::Greater => unreachable!("number of channel exceeded the maximum"),
                        Ordering::Equal => continue,
                        Ordering::Less => {
                            if neighbor.channels.is_empty() {
                                if neighbor.remaining_retry_ticks == 0 {
                                    neighbor.remaining_retry_ticks = CONN_ATTEMPT_TICKS;
                                } else {
                                    neighbor.remaining_retry_ticks -= 1;
                                    continue;
                                }
                            }
                        }
                    }

                    neighbor.num_pending_out_conn += 1;
                    retry_conn_tasks.push((addr, neighbor_public_key.clone()));
                }

                for (addr, neighbor_public_key) in retry_conn_tasks {
                    let neighbors = neighbors_for_task.clone();
                    let mut networker_sender = networker_sender_for_task.clone();

                    let new_channel = Channel::connect(
                        &addr,
                        &handle_for_task,
                        &neighbor_public_key,
                        &neighbors_for_task,
                        &networker_sender_for_task,
                        &sm_client_for_task
                    ).then(move |res| {
                        neighbors.lock().map_err(|_: ()| {
                            error!("failed to add new channel, would not allow retry conn");
                            ChannelError::FutMutex
                        }).and_then(move |mut neighbors| {
                            match neighbors.get_mut(&neighbor_public_key) {
                                None => {
                                    Err(ChannelError::Closed("unknown neighbor"))
                                }
                                Some(ref_neighbor) => {
                                    match res {
                                        Ok((channel_uid, channel_tx, channel)) => {
                                            if ref_neighbor.channels.is_empty() {
                                                let msg = ChannelerToNetworker::ChannelOpened(
                                                    ChannelOpened {
                                                        remote_public_key: neighbor_public_key,
                                                        locally_initialized: true,
                                                    }
                                                );
                                                if networker_sender.try_send(msg).is_err() {
                                                    return Err(ChannelError::SendToNetworkerFailed);
                                                }
                                            }
                                            ref_neighbor.channels.push(
                                                (channel_uid, channel_tx)
                                            );
                                            ref_neighbor.num_pending_out_conn -= 1;
                                            Ok(channel)
                                        }
                                        Err(e) => {
                                            ref_neighbor.num_pending_out_conn -= 1;
                                            Err(e)
                                        }
                                    }
                                }
                            }
                        })
                    });

                    let handle_for_channel = handle_for_task.clone();

                    handle_for_task.spawn(
                        new_channel.map_err(|e| {
                            error!("failed to attempt a new connection: {:?}", e);
                            ()
                        }).and_then(move |channel| {
                            handle_for_channel.spawn(channel.map_err(|_| ()));
                            Ok(())
                        })
                    );
                }

                Ok(())
            });

        self.handle.spawn(retry_conn_task.map_err(|_| ()));
    }

    #[inline]
    fn broadcast_tick(&self) {
        let handle = self.handle.clone();
        let networker_tx = self.inner_tx.clone();
        let mutex_neighbors = self.neighbors.clone();

        let task = self.neighbors.acquire(move |mut neighbors| {
            for (ref_public_key, mut neighbor) in neighbors.iter_mut() {
                for &(ref_channel_uuid, ref_channel_tx) in neighbor.channels.iter() {
                    let channel_uuid = ref_channel_uuid.clone();

                    let task = ref_channel_tx.clone().send(ToChannel::TimeTick).map_err(|_e| {
                        info!("failed to send ticks to channel: {:?}, removing", channel_uuid);

                        // The task to remove channel
                        let task_remove_channel = mutex_neighbors.acquire(|mut neighbors| {
                            match neighbors.get_mut(ref_public_key) {
                                None => {
                                    info!("nonexistent neighbor");
                                }
                                Some(neighbor) => {
                                    neighbor.channels.retain(|&(uuid, _)| {
                                        uuid != channel_uuid
                                    });

                                    info!("channel {:?} removed", channel_uuid);

                                    // Notify the Networker if there is no active channel
                                    if neighbor.channels.is_empty() {
                                        let msg = ChannelerToNetworker {
                                            remote_public_key: ref_public_key.clone(),
                                            channel_index: 0,
                                            event: ChannelEvent::Closed,
                                        };

                                        let task_notify = networker_tx
                                            .send(msg)
                                            .map_err(|_| {
                                                warn!("failed to notify networker");
                                            })
                                            .and_then(|_| Ok(()));

                                        handle.spawn(task_notify);
                                    }
                                }
                            }
                            Ok((neighbors, ()))
                        }).map_err(|_: AsyncMutexError<()>| {
                            info!("failed to remove dead channel: {:?}", channel_uuid);
                        });

                        handle.spawn(task_remove_channel);
                    });

                    handle.spawn(task);
                }
            }

            Ok((neighbors, ()))
        }).map_err(|_: AsyncMutexError<()>| {
            warn!("failed to broadcast tick event to channels");
        });

        self.handle.spawn(task);
    }

    // TODO: Consume all message before closing actually.
    #[inline]
    fn close(&mut self) {
        self.inner_rx.close();
        match mem::replace(&mut self.close_tx, None) {
            None => {
                error!("call close after close sender consumed, something go wrong");
            }
            Some(close_sender) => {
                if close_sender.send(()).is_err() {
                    error!("remote close handle deallocated, something may go wrong");
                }
            }
        }
    }
}

impl Future for TimerReader {
    type Item = ();
    type Error = TimerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        match self.close_rx.poll()? {
            Async::NotReady => (),
            Async::Ready(()) => {
                debug!("received the closing request, closing");

                self.close();

                return Ok(Async::Ready(()));
            }
        }

        loop {
            match self.inner_rx.poll() {
                Err(_) => {
                    error!("inner receiver error, closing");

                    self.close();

                    return Ok(Async::Ready(()));
                }
                Ok(item) => {
                    match item {
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        },
                        Async::Ready(None) => {
                            error!("inner receiver closed, closing");

                            self.close();

                            return Ok(Async::Ready(()));
                        }
                        Async::Ready(Some(FromTimer::TimeTick)) => {
                            self.retry_conn();
                            self.broadcast_tick();
                        }
                    }
                }
            }
        }
    }
}
