use std::mem;
use std::collections::HashMap;

use futures::sync::{mpsc, oneshot};
use futures::{Async, Poll, Future, Stream};

use futures_mutex::FutMutex;

use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use super::{ChannelerNeighbor, ToChannel};
use close_handle::{CloseHandle, create_close_handle};
use inner_messages::{NetworkerToChanneler, ChannelerNeighborInfo};

pub enum NetworkerReaderError {
    MessageReceiveFailed,
    RemoteCloseHandleClosed,
    FutMutex,
}

impl From<oneshot::Canceled> for NetworkerReaderError {
    #[inline]
    fn from(_e: oneshot::Canceled) -> NetworkerReaderError {
        NetworkerReaderError::RemoteCloseHandleClosed
    }
}

#[must_use = "futures do nothing unless polled"]
pub struct NetworkerReader {
    handle: Handle,
    neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>,

    inner_receiver: mpsc::Receiver<NetworkerToChanneler>,

    // For reporting the closing event
    close_sender:   Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_receiver: oneshot::Receiver<()>,
}

impl NetworkerReader {
    // Create a new `NetworkerReader`, return a new `NetworkerReader` with its `CloseHandle`.
    pub fn new(
        handle: Handle,
        receiver: mpsc::Receiver<NetworkerToChanneler>,
        neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>
    ) -> (CloseHandle, NetworkerReader) {
        let (close_handle, (close_sender, close_receiver)) = create_close_handle();

        let networker_reader = NetworkerReader {
            handle,
            neighbors,
            close_sender: Some(close_sender),
            close_receiver,
            inner_receiver: receiver,
        };

        (close_handle, networker_reader)
    }

    #[inline]
    fn add_neighbor(&self, info: ChannelerNeighborInfo) {
        let add_neighbor_task = self.neighbors.clone().lock()
            .map_err(|_| NetworkerReaderError::FutMutex)
            .and_then(move |mut neighbors| {
                neighbors.insert(
                    info.neighbor_address.neighbor_public_key.clone(),
                    ChannelerNeighbor {
                        info,
                        num_pending_out_conn: 0,
                        remaining_retry_ticks: 0, // Try a new connections immediately
                        channels: Vec::new(),
                    });
                Ok(())
            });

        self.handle.spawn(add_neighbor_task.map_err(|_| ()));
    }

    #[inline]
    fn del_neighbor(&self, public_key: PublicKey) {
        let del_neighbor_task = self.neighbors.clone().lock()
            .map_err(|_| NetworkerReaderError::FutMutex)
            .and_then(move |mut neighbors| {
                if neighbors.remove(&public_key).is_some() {
                    trace!("succeed in removing neighbor");
                } else {
                    warn!("attempt to remove a nonexistent neighbor: {:?}", public_key);
                }
                Ok(())
            });

        self.handle.spawn(del_neighbor_task.map_err(|_| ()));
    }

    #[inline]
    fn send_message(&self, token: u32, public_key: PublicKey, content: Vec<u8>) {
        let send_message_task = self.neighbors.clone().lock()
            .map_err(|_| NetworkerReaderError::FutMutex)
            .and_then(move |mut neighbors| {
                match neighbors.get_mut(&public_key) {
                    None => warn!(
                        "attempt to send message to a nonexistent neighbor: {:?}",
                        public_key
                    ),
                    Some(neighbors) => {
                        let channel_idx = (token as usize) % neighbors.channels.len();

                        let channel_sender = &mut neighbors.channels[channel_idx].1;
                        let msg_to_channel = ToChannel::SendMessage(content);

                        // FIXME: Use backpressure strategy here?
                        if channel_sender.try_send(msg_to_channel).is_err() {
                            error!("failed to send message to channel, message will be dropped!");
                        }
                    }
                }

                Ok(())
            });

        self.handle.spawn(send_message_task.map_err(|_| ()));
    }

    #[inline]
    fn set_max_channels(&self, public_key: PublicKey, max_channels: u32) {
        let set_max_channels_task = self.neighbors.clone().lock()
            .map_err(|_| NetworkerReaderError::FutMutex)
            .and_then(move |mut neighbors| {
                match neighbors.get_mut(&public_key) {
                    None => warn!(
                        "attempt to set max_channels of a nonexistent neighbor: {:?}",
                        public_key
                    ),
                    Some(neighbors) => {
                        neighbors.info.max_channels = max_channels;
                    }
                }

                Ok(())
            });

        self.handle.spawn(set_max_channels_task.map_err(|_| ()));
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

impl Future for NetworkerReader {
    type Item = ();
    type Error = NetworkerReaderError;

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
                    debug!("inner receiver error, closing");

                    self.close();

                    return Ok(Async::Ready(()));
                }
                Ok(item) => {
                    match item {
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        },
                        Async::Ready(None) => {
                            debug!("inner receiver closed, closing");

                            self.close();

                            return Ok(Async::Ready(()));
                        }
                        Async::Ready(Some(message)) => {
                            match message {
                                NetworkerToChanneler::AddNeighbor {
                                    neighbor_info
                                } => {
                                    self.add_neighbor(neighbor_info);
                                }
                                NetworkerToChanneler::RemoveNeighbor {
                                    neighbor_public_key
                                } => {
                                    self.del_neighbor(neighbor_public_key);
                                }
                                NetworkerToChanneler::SendChannelMessage {
                                    token,
                                    neighbor_public_key,
                                    content
                                } => {
                                    self.send_message(token, neighbor_public_key, content);
                                }
                                NetworkerToChanneler::SetMaxChannels {
                                    neighbor_public_key,
                                    max_channels
                                } => {
                                    self.set_max_channels(neighbor_public_key, max_channels);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
