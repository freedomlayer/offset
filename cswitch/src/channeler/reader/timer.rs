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
}

// TODO CR: Maybe we should produce the TimerReaderError::RemoteCloseHandleClosed at the right
// place, instead of having it here?
// I could understand this kind of error conversion if it meant something generic, but it seems
// like this conversion means something local to a specific place in the code. It will probably be
// easier to understand what happens here if this code will be moved to the relevant place.
//
// As an example, what if we had more than one oneshots used inside the TimerReader in the future?
// For one of those oneshots we will have TimerReaderError::RemoteOneshot1Closed, and for the
// other one we will have TimerReaderError::RemoteOneshot2Closed. We won't be able to use the impl
// From<oneshot::Canceled> for TimerReaderError> in that case.
impl From<oneshot::Canceled> for TimerReaderError {
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
    // TODO CR: Maybe we should change the name of this function to something else?
    // We might be going against the Rust convention by having Object::new() return something that
    // is not of type Object. We might be able to pick a different name to this function. What do
    // you think?
    pub fn new(
        handle: Handle,
        timer_receiver: mpsc::Receiver<FromTimer>,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client: SecurityModuleClient,
        // TODO CR: I think that we should have an alias for this neighbors type, or define a new
        // type (structure) for this, as we type it in many places inside the Channeler code.
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

    /// Spawn a connection attempt to any neighbor for which we are the active side in the
    /// relationship. We attempt to connect to every neighbor every once in a while.
    fn retry_conn(&self) {
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = self.neighbors.clone();
        let sm_client_for_task = self.sm_client.clone();
        let networker_sender_for_task = self.inner_tx.clone();

        // TODO CR: We acquire neighbors for too long here.
        let retry_conn_task = self.neighbors
            .acquire(move |mut neighbors| {
                let mut need_retry = Vec::new();

                for (neighbor_public_key, mut neighbor) in &mut neighbors.iter_mut() {
                    if neighbor.info.socket_addr.is_some() && neighbor.num_pending == 0 {
                        // TODO CR: Maybe we should move the logic of this scope to a separate
                        // function? It seems like this part tries to decide whether we should
                        // continue, or execute the next code after this scope ends.
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

                        // TODO CR: We can implement this logic as a separate function
                        let mut channel_index: Option<u32> = None;
                        // Find the minimum index which isn't in use
                        for index in 0..neighbor.info.max_channels {
                            if neighbor.channels.get(&index).is_none() {
                                channel_index = Some(index);
                                break;
                            }
                        }

                        if let Some(index) = channel_index {
                            let addr = neighbor.info.socket_addr.clone().unwrap();
                            need_retry.push((addr, neighbor_public_key.clone(), index));
                            // TODO CR: Is this the right place to increase num_pending? Maybe we
                            // should do it right before we open the new Channel? I need to read
                            // the rest of the code in this file to be sure about this.
                            neighbor.num_pending += 1;
                        }
                    }
                }

                // TODO CR: At this point we don't use the acquired neighbors anymore. We probably
                // keep the AsyncMutex acquired for too long.  We should close the scope of the
                // acquire here, possibly use an "and_then" clause to add the next code:

                // TODO CR: Move the following logic into a separate function. retry_con is pretty
                // long, if we separate it to smaller parts we can make it easier to read and test.
                for (addr, neighbor_public_key, index) in need_retry {
                    let neighbors = neighbors_for_task.clone();
                    let mut networker_sender = networker_sender_for_task.clone();

                    let new_channel = Channel::connect(
                        // TODO CR: addr: SockAddr implements Copy, we might be able to move it without a
                        // reference.
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
                                    // TODO CR: Use Enum to denote closing reason instead of using
                                    // a string.
                                    None => Err(ChannelError::Closed("Can't find this neighbor")),
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
                                            // TODO CR: We might be able to use the "?" syntax here
                                            // for conn_result.
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
                                // TODO CR: Can we really arrive here? Or do we have this because
                                // of the AsyncMutex design?
                                error!("Failed to initialize a new connection: {:?}", e);
                                ()
                            })
                    });

                    let handle_for_channel = handle_for_task.clone();

                    // TODO CR: Why do we spawn twice here? 
                    // I think that we may drop the second spawn.
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

        // TODO CR: I think that we don't really need a task spawn for this.
        // retry_conn_task doesn't really block. It just acquires neighbors, this should happen
        // instantly.
        //
        // We are probably better off if we wait for retry_conn_task to end instead of spawning it.
        // What do you think?
        self.handle.spawn(retry_conn_task);
    }

    fn broadcast_tick(&self) {
        let handle_for_task = self.handle.clone();
        let networker_sender_for_task = self.inner_tx.clone();
        let neighbor_for_task = self.neighbors.clone();


        // TODO CR: We are allocating here a new task for every time tick sent to any channel.
        // I remember that this is how I initially implemented this part.
        // I think that we might be able to implement this so that we don't need to allocate new
        // tasks every time, I propose the following implementation:
        //
        // We will have a vector Senders, to be able to send Time Ticks to all the Channels.
        // We then go over all the vector and attempt to send a time tick to all of them using
        // try_send. Some of them might be NotReady, in that case we just give up on them, and they
        // are going to miss a time tick. 
        //
        // This should simplify the implementation here, and allow us to code this part without
        // spawning new tasks. Please tell me what you think about this when you read this.

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
    // TODO CR: What messages do we need to consume here? The buffered messages inside the Streams?
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

// TODO CR: I think that we might be able to rewrite this as a for_each over a stream, where the
// stream is the result of select() over inner_rx and close_rx. 
// I admit that I'm not sure if it will make the code shorter in this case, but if this code ever
// gets bigger this might be a good idea.
impl Future for TimerReader {
    type Item = ();
    type Error = TimerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        // Check if we received a closing request:
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
