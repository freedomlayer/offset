use std::mem;
use std::collections::HashMap;

use bytes::Bytes;
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use utils::{AsyncMutex, AsyncMutexError, CloseHandle};
use channeler::types::{ChannelerNeighbor, ChannelerNeighborInfo};
use channeler::messages::ToChannel;

use networker::messages::NetworkerToChanneler;

use crypto::identity::PublicKey;

pub enum NetworkerReaderError {
    MessageReceiveFailed,
    RemoteCloseHandleClosed,
    // TODO CR: I think that this is error is unused:
    FutMutex,
}

impl From<oneshot::Canceled> for NetworkerReaderError {
    // TODO CR: Unless we know about a specific performance problem with this function, I don't
    // think that we need to tell the compiler to inline this.
    #[inline]
    fn from(_e: oneshot::Canceled) -> NetworkerReaderError {
        NetworkerReaderError::RemoteCloseHandleClosed
    }
}

// TODO CR: What happens if we don't have this? I thought that all Futures have this by default.
#[must_use = "futures do nothing unless polled"]
pub struct NetworkerReader {
    handle: Handle,
    neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,

    inner_rx: mpsc::Receiver<NetworkerToChanneler>,

    // For reporting the closing event
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl NetworkerReader {
    // TODO CR:
    // Maybe we should rename this function? The Rust convention is that Object::new() returns
    // Object type, but here we return a tuple.
    
    // Create a new `NetworkerReader`, return a new `NetworkerReader` with its `CloseHandle`.
    pub fn new(
        handle: Handle,
        receiver: mpsc::Receiver<NetworkerToChanneler>,
        // TODO CR: We can probably alias the neighbors type or give it its own type, in order to
        // avoid repeating this type.
        neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    ) -> (CloseHandle, NetworkerReader) {
        let (close_handle, (close_tx, close_rx)) = CloseHandle::new();

        let reader = NetworkerReader {
            handle,
            neighbors,
            close_tx: Some(close_tx),
            close_rx: close_rx,
            inner_rx: receiver,
        };

        (close_handle, reader)
    }

    // TODO CR: See other comments about using #[inline]
    #[inline]
    fn add_neighbor(&self, info: ChannelerNeighborInfo) {
        let task = self.neighbors
            .acquire(|mut neighbors| {
                neighbors.insert(
                    info.public_key.clone(),
                    ChannelerNeighbor {
                        info,
                        num_pending: 0,
                        retry_ticks: 0,
                        channels: HashMap::new(),
                    },
                );
                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
                info!("failed to add neighbor");
            });

        // TODO CR: Maybe we can avoid spawning a new task here. See longer explanation in
        // del_neighbor.
        self.handle.spawn(task);
    }

    // TODO CR: See other comments about using #[inline]
    #[inline]
    fn del_neighbor(&self, public_key: PublicKey) {
        let task = self.neighbors
            .acquire(move |mut neighbors| {
                if neighbors.remove(&public_key).is_some() {
                    info!("neighbor {:?} removed", public_key);
                    // TODO CR: Will this close all current TCP connections with the neighbor?
                    // If there are any Channels of communication with this neighbor 
                    // currently working, do we need to close them here?
                } else {
                    info!("nonexistent neighbor: {:?}", public_key);
                }
                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
                info!("failed to remove neighbor");
            });


        // TODO CR: I don't think that we need to spawn a task here.
        // Here are my reasons:
        //
        // 1. del_neighbor barely does anything async related.  Theoretically we don't even need
        //    AsyncMutex over neighbors here, as all of the futures that touch it live in the same
        //    thread, and it is never possible for two of them to run at the same time.
        //
        // 2. We lose control over the order in which things happen. It makes it hard for me to reason
        //    about the order that things happen. For example: Assume that the Networker has just
        //    sent two requests, one to delete a neighbor and one to add a neighbor. If we spawn
        //    tasks to do those things, we have the risk of doing the two tasks (adding and
        //    deleting a neighbor) in the wrong order.
        //
        // Instead, I think that we have the following options:
        //
        // 1. Use combinators. We wait until the acquiring of neighbors and its modification is
        //    done, and only then we move on to process the next message. This could be done as a
        //    pattern of applying for_each over a Stream of events.
        //
        // 2. Keep the current operation in progress inside the state machine. We won't start doing
        //    other operations until the current operation is not done.
        //
        self.handle.spawn(task);
    }

    // TODO CR: See other comments about using #[inline]
    #[inline]
    fn send_message(&self, index: u32, public_key: PublicKey, content: Bytes) {
        let task = self.neighbors
            .acquire(move |mut neighbors| {
                match neighbors.get_mut(&public_key) {
                    None => {
                        warn!(
                            "nonexistent neighbor: {:?}, message will be discarded",
                            public_key
                        );
                    }
                    Some(neighbor) => {
                        match neighbor.channels.get_mut(&index) {
                            None => {
                                warn!(
                                    "unknown channel index: {:?}, message will be discarded",
                                    index
                                );
                            }
                            Some(sender) => {
                                let message = ToChannel::SendMessage(content);

                                if sender.try_send(message).is_err() {
                                    error!("failed to send message to channel, message will be dropped!");
                                }
                            }
                        }
                    }
                }

                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
                info!("failed to pass send message request to channel");
            });

        // TODO CR: I think that here spawning a task is actually the correct thing to do , because
        // it is possible that the neighbor sender is blocked, and we don't want to block the whole
        // NetworkerReader Future because of that. 
        //
        // However, it seems like we allocate a new task for every incoming message. This could be
        // a very big overhead. I have an idea of how to fix this, but it requires a small
        // change to the messages interface between Channeler and Networker.
        //
        // Refactor Proposal
        // -----------------
        //
        // Whenever a new channel is opened, Channeler will send Networker a ChannelEvent::Opened.
        // ChannelEvent::Opened will contain an mpsc Sender and an mpsc Receiver.
        // The Receiver will allow receiving messages from the Channel. The Sender will allow
        // sending messages to the Channel.
        // 
        // Using this method the Networker will be able to experience backpressure for every new
        // Channel separately, and we won't need to spawn a new task for every message.
        //
        // Please tell me you opinion about this, you might have an idea to improve this.
        //
        self.handle.spawn(task);
    }

    // TODO CR: See other comments about using #[inline]
    #[inline]
    fn set_max_channels(&self, public_key: PublicKey, max_channels: u32) {
        let task = self.neighbors
            .acquire(move |mut neighbors| {
                match neighbors.get_mut(&public_key) {
                    None => {
                        info!("nonexistent neighbor: {:?}", public_key);
                    }
                    Some(neighbor) => {
                        neighbor.info.max_channels = max_channels;
                    }
                }
                Ok((neighbors, ()))
            })
            .map_err(|_: AsyncMutexError<()>| {
                info!("failed to set maximum amount of channels");
            });

        // TODO CR: We also need to close at this point all current TCP connections that have an
        // index larger than max_channels (This could happen if we lowered max_channels).

        // TODO CR: I don't think that we need to spawn a task here.
        // See earlier comments about this.
        self.handle.spawn(task);
    }

    // TODO: Consume all message before closing actually.
    // TODO CR: See other comments about using #[inline]
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

// TODO CR: I think that we might be able to implement this state machine as a select() over
// (inner_rx and close_rx), together with for_each over the resulting Stream. What do you think?
impl Future for NetworkerReader {
    type Item = ();
    type Error = NetworkerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        // Check if we have received a request to close:
        match self.close_rx.poll()? {
            Async::NotReady => (),
            Async::Ready(()) => {
                info!("close request received, closing");
                self.close();
                return Ok(Async::Ready(()));
            }
        }

        loop {
            match self.inner_rx.poll() {
                Err(_) => {
                    debug!("inner receiver error, closing");

                    self.close();

                    return Ok(Async::Ready(()));
                }
                Ok(item) => match item {
                    Async::NotReady => {
                        return Ok(Async::NotReady);
                    }
                    Async::Ready(None) => {
                        debug!("inner receiver closed, closing");

                        self.close();

                        return Ok(Async::Ready(()));
                    }
                    Async::Ready(Some(message)) => match message {
                        NetworkerToChanneler::AddNeighbor { neighbor_info } => {
                            self.add_neighbor(neighbor_info);
                        }
                        NetworkerToChanneler::RemoveNeighbor {
                            neighbor_public_key,
                        } => {
                            self.del_neighbor(neighbor_public_key);
                        }
                        NetworkerToChanneler::SendChannelMessage {
                            neighbor_public_key,
                            channel_index,
                            content,
                        } => {
                            self.send_message(channel_index, neighbor_public_key, content);
                        }
                        NetworkerToChanneler::SetMaxChannels {
                            neighbor_public_key,
                            max_channels,
                        } => {
                            self.set_max_channels(neighbor_public_key, max_channels);
                        }
                    },
                },
            }
        }
    }
}
