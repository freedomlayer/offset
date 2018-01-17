use std::mem;
use std::cmp::Ordering;
use std::collections::HashMap;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use utils::{AsyncMutex, AsyncMutexError, CloseHandle};
use crypto::identity::PublicKey;
use security_module::client::SecurityModuleClient;

use timer::messages::FromTimer;
use channeler::channel::{Channel, ChannelError};
use channeler::types::ChannelerNeighbor;
use channeler::messages::{ChannelEvent, ChannelerToNetworker, ToChannel};

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
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl TimerReader {
    pub fn new(
        handle: Handle,
        timer_receiver: mpsc::Receiver<FromTimer>,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client: SecurityModuleClient,
        neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    ) -> (CloseHandle, TimerReader) {
        let (close_handle, (close_tx, close_rx)) = CloseHandle::new();

        let timer_reader = TimerReader {
            handle,
            sm_client,
            neighbors,
            close_tx: Some(close_tx),
            close_rx: close_rx,
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

        let retry_conn_task = self.neighbors
            .acquire(move |mut neighbors| {
                let mut need_retry = Vec::new();

                for (neighbor_public_key, mut neighbor) in &mut neighbors.iter_mut() {
                    if neighbor.info.socket_addr.is_some() && neighbor.num_pending == 0 {
                        match neighbor
                            .channels
                            .len()
                            .cmp(&(neighbor.info.max_channels as usize))
                        {
                            Ordering::Equal => continue,
                            Ordering::Greater => {
                                unreachable!("number of channel exceeded the maximum")
                            }
                            Ordering::Less => {
                                if neighbor.channels.is_empty() {
                                    if neighbor.retry_ticks == 0 {
                                        neighbor.retry_ticks = CONN_ATTEMPT_TICKS;
                                    } else {
                                        neighbor.retry_ticks -= 1;
                                        continue;
                                    }
                                }
                            }
                        }

                        let mut channel_index: Option<u32> = None;
                        // Find the minimum index which isn't is used
                        for index in 0..neighbor.info.max_channels {
                            if neighbor.channels.get(&index).is_none() {
                                channel_index = Some(index);
                                break;
                            }
                        }

                        if let Some(index) = channel_index {
                            let addr = neighbor.info.socket_addr.clone().unwrap();
                            need_retry.push((addr, neighbor_public_key.clone(), index));
                            neighbor.num_pending += 1;
                        }
                    }
                }

                for (addr, neighbor_public_key, index) in need_retry {
                    let neighbors = neighbors_for_task.clone();
                    let mut networker_sender = networker_sender_for_task.clone();

                    let new_channel = Channel::connect(
                        &addr,
                        &neighbor_public_key,
                        index,
                        &neighbors_for_task,
                        &networker_sender_for_task,
                        &sm_client_for_task,
                        &handle_for_task,
                    ).then(move |conn_result| {
                        neighbors
                            .acquire(move |mut neighbors| {
                                let res = match neighbors.get_mut(&neighbor_public_key) {
                                    None => Err(ChannelError::Closed("can't find this neighbor")),
                                    Some(neighbor) => {
                                        neighbor.num_pending -= 1;
                                        match conn_result {
                                            Ok((channel_index, channel_tx, channel)) => {
                                                let msg = ChannelerToNetworker {
                                                    remote_public_key: neighbor_public_key,
                                                    channel_index: channel_index,
                                                    event: ChannelEvent::Opened,
                                                };

                                                if networker_sender.try_send(msg).is_err() {
                                                    Err(ChannelError::SendToNetworkerFailed)
                                                } else {
                                                    neighbor
                                                        .channels
                                                        .insert(channel_index, channel_tx);
                                                    Ok(channel)
                                                }
                                            }
                                            Err(e) => Err(e),
                                        }
                                    }
                                };

                                match res {
                                    Ok(channel) => Ok((neighbors, channel)),
                                    Err(e) => Err((Some(neighbors), e)),
                                }
                            })
                            .map_err(|e| {
                                error!("failed to attempt a new connection: {:?}", e);
                                ()
                            })
                    });

                    let handle_for_channel = handle_for_task.clone();

                    handle_for_task.spawn(new_channel.and_then(move |channel| {
                        handle_for_channel.spawn(channel.map_err(|_| ()));
                        Ok(())
                    }));
                }

                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
                error!("retry conn failed");
            });

        self.handle.spawn(retry_conn_task);
    }

    #[inline]
    fn broadcast_tick(&self) {
        let handle_for_task = self.handle.clone();
        let networker_sender_for_task = self.inner_tx.clone();
        let neighbor_for_task = self.neighbors.clone();

        let task = self.neighbors
            .acquire(move |neighbors| {
                for (public_key, neighbor) in neighbors.iter() {
                    for (channel_index, channel_tx) in neighbor.channels.iter() {
                        let channel_index = *channel_index;
                        let handle_for_subtask = handle_for_task.clone();
                        let neighbor_public_key = public_key.clone();
                        let neighbors_for_subtask = neighbor_for_task.clone();
                        let networker_sender_for_subtask = networker_sender_for_task.clone();

                        let task = channel_tx
                            .clone()
                            .send(ToChannel::TimeTick)
                            .map_err(move |_e| {
                                info!(
                                    "failed to send ticks to channel: {:?}, removing",
                                    channel_index
                                );

                                neighbors_for_subtask
                                    .acquire(move |mut neighbors| {
                                        match neighbors.get_mut(&neighbor_public_key) {
                                            None => {
                                                info!("nonexistent neighbor");
                                            }
                                            Some(neighbor) => {
                                                neighbor
                                                    .channels
                                                    .retain(|index, _| *index != channel_index);

                                                info!("channel {:?} removed", channel_index);

                                                let msg = ChannelerToNetworker {
                                                    remote_public_key: neighbor_public_key,
                                                    channel_index: channel_index,
                                                    event: ChannelEvent::Closed,
                                                };

                                                let task_notify = networker_sender_for_subtask
                                                    .send(msg)
                                                    .map_err(|_| {
                                                        warn!("failed to notify networker");
                                                    })
                                                    .and_then(|_| Ok(()));

                                                handle_for_subtask.spawn(task_notify);
                                            }
                                        }
                                        Ok((neighbors, ()))
                                    })
                                    .map_err(move |_: AsyncMutexError<()>| {
                                        info!("failed to remove dead channel: {:?}", channel_index);
                                    })
                            })
                            .then(|_| Ok(()));

                        handle_for_task.spawn(task);
                    }
                }

                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
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
                Ok(item) => match item {
                    Async::NotReady => {
                        return Ok(Async::NotReady);
                    }
                    Async::Ready(None) => {
                        error!("inner receiver closed, closing");

                        self.close();

                        return Ok(Async::Ready(()));
                    }
                    Async::Ready(Some(FromTimer::TimeTick)) => {
                        self.retry_conn();
                        self.broadcast_tick();
                    }
                },
            }
        }
    }
}
