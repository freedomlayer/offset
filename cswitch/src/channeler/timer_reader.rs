use std::mem;
use std::cmp::Ordering;
use std::collections::HashMap;

use futures::sync::{mpsc, oneshot};
use futures::{Async, Poll, Future, Stream, Sink};

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use inner_messages::{FromTimer, ChannelerToNetworker, ChannelClosed};
use close_handle::{CloseHandle, create_close_handle};
use security_module::security_module_client::SecurityModuleClient;
use futures_mutex::FutMutex;

use super::channel::Channel;
use super::{ChannelerNeighbor, ToChannel};

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
    neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    sm_client: SecurityModuleClient,

    inner_sender: mpsc::Sender<ChannelerToNetworker>,
    inner_receiver: mpsc::Receiver<FromTimer>,

    // For reporting the closing event
    close_sender:   Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_receiver: oneshot::Receiver<()>,
}

impl TimerReader {
    pub fn new(
        handle:           Handle,
        timer_receiver:   mpsc::Receiver<FromTimer>,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client:        SecurityModuleClient,
        neighbors:        FutMutex<HashMap<PublicKey, ChannelerNeighbor>>
    ) -> (CloseHandle, TimerReader) {
        let (close_handle, (close_sender, close_receiver)) = create_close_handle();

        let timer_reader = TimerReader {
            handle,
            sm_client,
            neighbors,
            close_sender: Some(close_sender),
            close_receiver,
            inner_sender: networker_sender,
            inner_receiver: timer_receiver,
        };

        (close_handle, timer_reader)
    }

    #[inline]
    fn retry_conn(&self) {
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = self.neighbors.clone();
        let sm_client_for_task = self.sm_client.clone();
        let networker_sender_for_task = self.inner_sender.clone();

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
                    let new_channel = Channel::connect(
                        &addr,
                        &handle_for_task,
                        &neighbor_public_key,
                        &neighbors_for_task,
                        &networker_sender_for_task,
                        &sm_client_for_task
                    ).map_err(|_| ());

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
        // FIXME: Can we reduce clone here?
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = self.neighbors.clone();
        let networker_sender_for_task = self.inner_sender.clone();

        let broadcast_tick_task = self.neighbors.clone().lock()
            .map_err(|_: ()| TimerReaderError::FutMutex)
            .and_then(move |neighbors| {
                for (neighbor_public_key, mut neighbor) in &mut neighbors.iter() {
                    for &(ref channel_id, ref channel_sender) in &neighbor.channels {

                        let target_id = channel_id.clone();
                        let neighbors = neighbors_for_task.clone();
                        let neighbor_public_key = neighbor_public_key.clone();
                        let networker_sender = networker_sender_for_task.clone();

                        let handle_inner1 = handle_for_task.clone();
                        let handle_inner2 = handle_for_task.clone();
                        let send_tick_task = channel_sender.clone().send(ToChannel::TimeTick);

                        handle_for_task.spawn(send_tick_task.map_err(move |_e| {
                            debug!("channel dead, removing {:?}", target_id);

                            handle_inner1.spawn(neighbors.clone().lock()
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
            });

        self.handle.spawn(broadcast_tick_task.map_err(|_| ()));
    }

    // TODO: Consume all message before closing actually.
    #[inline]
    fn close(&mut self) {
        self.inner_receiver.close();
        match mem::replace(&mut self.close_sender, None) {
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

        match self.close_receiver.poll()? {
            Async::NotReady => (),
            Async::Ready(()) => {
                debug!("received the closing request, closing");

                self.close();

                return Ok(Async::Ready(()));
            }
        }

        loop {
            match self.inner_receiver.poll() {
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
