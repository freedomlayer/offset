use std::{mem, cell::RefCell, rc::Rc};

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use utils::CloseHandle;
use security_module::client::SecurityModuleClient;

use timer::messages::FromTimer;
use channeler::channel::{Channel, ChannelError};
use channeler::types::{ChannelerNeighbor, NeighborsTable};
use channeler::messages::{ChannelEvent, ChannelerToNetworker, ToChannel};

const CONN_ATTEMPT_TICKS: usize = 120;

#[derive(Debug)]
pub enum TimerReaderError {
    // TimerReceiveFailed,
    RemoteCloseHandleCanceled,
    // SendToNetworkerFailed,
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
        TimerReaderError::RemoteCloseHandleCanceled
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct TimerReader {
    handle: Handle,
    neighbors: Rc<RefCell<NeighborsTable>>,
    sm_client: SecurityModuleClient,

    timer_receiver: mpsc::Receiver<FromTimer>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,

    // For reporting the closing event
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl TimerReader {
    pub fn new(
        timer_receiver: mpsc::Receiver<FromTimer>,
        handle: &Handle,
        networker_sender: mpsc::Sender<ChannelerToNetworker>,
        sm_client: SecurityModuleClient,
        neighbors: Rc<RefCell<NeighborsTable>>,
    ) -> (CloseHandle, TimerReader) {
        let (close_handle, (close_tx, close_rx)) = CloseHandle::new();

        let timer_reader = TimerReader {
            handle: handle.clone(),
            sm_client,
            neighbors,
            close_tx: Some(close_tx),
            close_rx,
            networker_sender,
            timer_receiver,
        };

        (close_handle, timer_reader)
    }

    /// Whether needs to retry connection.
    fn needs_retry(neighbor: &ChannelerNeighbor) -> bool {
        if neighbor.info.socket_addr.is_some() && neighbor.num_pending == 0 {
            if neighbor.channels.len() < neighbor.info.max_channels as usize {
                return neighbor.retry_ticks == 0;
            }
        }

        false
    }

    fn min_unused_channel_index(neighbor: &ChannelerNeighbor) -> Option<u32> {
        let mut channel_index: Option<u32> = None;
        for index in 0..neighbor.info.max_channels {
            if neighbor.channels.get(&index).is_none() {
                channel_index = Some(index);
                break;
            }
        }
        channel_index
    }

    /// Spawn a connection attempt to any neighbor for which we are the active side in the
    /// relationship. We attempt to connect to every neighbor every once in a while.
    fn retry_conn(&self) {
        let handle_for_task = self.handle.clone();
        let neighbors_for_task = self.neighbors.clone();
        let sm_client_for_task = self.sm_client.clone();
        let networker_sender_for_task = self.networker_sender.clone();

        // TODO: Extract the following part as a function
        let mut need_retry = Vec::new();
        {
            for (neighbor_public_key, mut neighbor) in &mut self.neighbors.borrow_mut().iter_mut() {
                // TODO: Use a separate function to do this.
                if neighbor.retry_ticks == 0 {
                    neighbor.retry_ticks = CONN_ATTEMPT_TICKS;
                } else {
                    neighbor.retry_ticks -= 1;
                }
                if TimerReader::needs_retry(neighbor) {
                    let mut channel_index = TimerReader::min_unused_channel_index(neighbor);

                    if let Some(index) = channel_index {
                        let addr = neighbor.info.socket_addr.clone().unwrap();
                        need_retry.push((addr, neighbor_public_key.clone(), index));
                        // TODO CR: Is this the right place to increase num_pending?
                        // Maybe we should do it right before we open the new Channel?
                        // I need to read the rest of the code in this file to be sure about this.
                        neighbor.num_pending += 1;
                    }
                }
            }
        }

        for (addr, neighbor_public_key, channel_index) in need_retry {
            let neighbors = neighbors_for_task.clone();
            let mut networker_sender = networker_sender_for_task.clone();

            let new_channel = Channel::connect(
                &addr,
                &neighbor_public_key,
                channel_index,
                Rc::clone(&neighbors_for_task),
                &networker_sender_for_task,
                &sm_client_for_task,
                &handle_for_task,
            ).then(move |conn_result| {
                match neighbors.borrow_mut().get_mut(&neighbor_public_key) {
                    None => Err(ChannelError::Closed("Can't find this neighbor")),
                    Some(neighbor) => {
                        neighbor.num_pending -= 1;

                        let (channel_index, channel_tx, channel) = conn_result?;

                        let msg = ChannelerToNetworker {
                            remote_public_key: neighbor_public_key,
                            channel_index,
                            event: ChannelEvent::Opened,
                        };

                        if networker_sender.try_send(msg).is_err() {
                            Err(ChannelError::SendToNetworkerFailed)
                        } else {
                            neighbor.channels.insert(channel_index, channel_tx);
                            Ok(channel)
                        }
                    }
                }
            });

            handle_for_task.spawn(
                new_channel
                    .map_err(|e| {
                        error!("failed to initialize a new connection: {:?}", e);
                    })
                    .and_then(|channel| {
                        channel.map_err(|e| {
                            warn!("channel closed: {:?}", e);
                        })
                    }),
            );
        }
    }

    fn broadcast_tick(&self) {
        let handle_for_task = self.handle.clone();
        let networker_sender_for_task = self.networker_sender.clone();
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

        for (neighbor_public_key, neighbor) in self.neighbors.borrow().iter() {
            for (channel_index, channel_tx) in neighbor.channels.iter() {
                let channel_index = *channel_index;
                let handle_for_subtask = handle_for_task.clone();
                let remote_public_key = neighbor_public_key.clone();
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
                        match neighbors_for_subtask
                            .borrow_mut()
                            .get_mut(&remote_public_key)
                        {
                            None => info!("nonexistent neighbor"),
                            Some(neighbor) => {
                                neighbor.channels.retain(|index, _| *index != channel_index);

                                info!("channel {:?} removed", channel_index);

                                let msg = ChannelerToNetworker {
                                    remote_public_key,
                                    channel_index,
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
                    })
                    .and_then(|_| Ok(()));

                handle_for_task.spawn(task);
            }
        }
    }

    // TODO: Consume all message before closing actually.
    // TODO CR: What messages do we need to consume here? The buffered messages inside the Streams?
    fn close(&mut self) {
        self.timer_receiver.close();
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
            match self.timer_receiver.poll() {
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
