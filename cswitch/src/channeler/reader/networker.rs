use std::{mem, cell::RefCell, rc::Rc};

use bytes::Bytes;
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use channeler::messages::ToChannel;
use channeler::types::{ChannelerNeighbor, ChannelerNeighborInfo, NeighborsTable};
use utils::{CloseHandle};

use networker::messages::NetworkerToChanneler;

use crypto::identity::PublicKey;

pub enum NetworkerReaderError {
    // MessageReceiveFailed,
    RemoteCloseHandleCanceled,
}

#[must_use = "futures do nothing unless polled"]
pub struct NetworkerReader {
    handle:    Handle,
    neighbors: Rc<RefCell<NeighborsTable>>,

    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,

    // For reporting the closing event
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl NetworkerReader {
    // TODO: Rename this function？
    // Create a new `NetworkerReader`, return a new `NetworkerReader` with its
    // `CloseHandle`.
    pub fn new(
        networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
        handle: &Handle,
        neighbors: Rc<RefCell<NeighborsTable>>,
    ) -> (CloseHandle, NetworkerReader) {
        let (close_handle, (close_tx, close_rx)) = CloseHandle::new();

        let networker_reader = NetworkerReader {
            handle: handle.clone(),
            neighbors,
            close_tx: Some(close_tx),
            close_rx,
            networker_receiver,
        };

        (close_handle, networker_reader)
    }

    /// Add neighbor relation.
    fn add_neighbor(&self, info: ChannelerNeighborInfo) {
        self.neighbors.borrow_mut().insert(
            info.public_key.clone(),
            ChannelerNeighbor {
                info,
                channel: None,
                num_pending: 0,
                retry_ticks: 0,
            },
        );
    }

    /// Remove neighbor relation.
    ///
    /// In this case, we just remove the `ToChannel` senders, if any one hold a clone
    /// of the sender, the channel perceive it should close ONLY all senders have gone.
    /// In other word, the scheduled jobs would continue and the channel would be closed
    /// until all scheduled jobs done!
    fn del_neighbor(&self, public_key: PublicKey) {
        if self.neighbors.borrow_mut().remove(&public_key).is_some() {
            info!("neighbor {:?} removed", public_key);
        } else {
            info!("nonexistent neighbor: {:?}", public_key);
        }
    }

    /// Send channel message via channel with index `channel_index`.
    fn send_message(&self, remote_public_key: PublicKey, content: Bytes) {
        let channel_sender = self.neighbors
            .borrow()
            .get(&remote_public_key)
            .and_then(|neighbor| neighbor.channel.clone());

        match channel_sender {
            None => info!("no such channel, message will be discarded"),
            Some(sender) => {
                let message = ToChannel::SendMessage(content);

                self.handle.spawn(
                    sender
                        .send(message)
                        .map_err(|_| {
                            warn!("failed to send message to channel, message will be discarded");
                        })
                        .then(|_| Ok(())),
                );
            },
        }

        // self.handle.spawn(task);

        // TODO CR: I think that here spawning a task is actually the correct thing to
        // do , because it is possible that the neighbor channel_sender is
        // blocked, and we don't want to block the whole NetworkerReader Future
        // because of that.
        //
        // However, it seems like we allocate a new task for every incoming message.
        // This could be a very big overhead. I have an idea of how to fix
        // this, but it requires a small change to the messages interface
        // between Channeler and Networker.
        //
        // Refactor Proposal
        // -----------------
        //
        // Whenever a new channel is opened, Channeler will send Networker a
        // ChannelEvent::Opened. ChannelEvent::Opened will contain an mpsc
        // Sender and an mpsc Receiver. The Receiver will allow receiving
        // messages from the Channel. The Sender will allow sending messages to
        // the Channel.
        //
        // Using this method the Networker will be able to experience backpressure for
        // every new Channel separately, and we won't need to spawn a new task
        // for every message.
        //
        // Please tell me you opinion about this, you might have an idea to improve
        // this.
        //
    }

    // TODO: Consume all message before closing actually.
    fn close(&mut self) {
        self.networker_receiver.close();
        match mem::replace(&mut self.close_tx, None) {
            None => {
                error!("call close after close sender consumed, something go wrong");
            },
            Some(close_sender) => {
                if close_sender.send(()).is_err() {
                    error!("remote close handle deallocated, something may go wrong");
                }
            },
        }
    }
}

// TODO CR: I think that we might be able to implement this state machine as a
// select() over (inner_rx and close_rx), together with for_each over the
// resulting Stream. What do you think?
impl Future for NetworkerReader {
    type Item = ();
    type Error = NetworkerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

        // Check if we have received a request to close:
        match self.close_rx
            .poll()
            .map_err(|_| NetworkerReaderError::RemoteCloseHandleCanceled)?
        {
            Async::NotReady => (),
            Async::Ready(()) => {
                info!("close request received, closing");
                self.close();
                return Ok(Async::Ready(()));
            },
        }

        loop {
            match self.networker_receiver.poll() {
                Err(_) => {
                    debug!("inner receiver error, closing");

                    self.close();

                    return Ok(Async::Ready(()));
                },
                Ok(item) => match item {
                    Async::NotReady => {
                        return Ok(Async::NotReady);
                    },
                    Async::Ready(None) => {
                        debug!("inner receiver closed, closing");

                        self.close();

                        return Ok(Async::Ready(()));
                    },
                    Async::Ready(Some(message)) => match message {
                        NetworkerToChanneler::AddNeighbor { info } => {
                            self.add_neighbor(info);
                        },
                        NetworkerToChanneler::RemoveNeighbor {
                            neighbor_public_key,
                        } => {
                            self.del_neighbor(neighbor_public_key);
                        },
                        NetworkerToChanneler::SendChannelMessage {
                            neighbor_public_key,
                            content,
                        } => {
                            self.send_message(neighbor_public_key, content);
                        },
                    },
                },
            }
        }
    }
}
