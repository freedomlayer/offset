use std::mem;
use std::collections::HashMap;

use bytes::Bytes;
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};

use tokio_core::reactor::Handle;

use async_mutex::{AsyncMutex, AsyncMutexError};
use channeler::types::{ChannelerNeighbor, ChannelerNeighborInfo};
use channeler::messages::ToChannel;

use networker::messages::NetworkerToChanneler;

use crypto::identity::PublicKey;
use close_handle::{CloseHandle, create_close_handle};

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
    neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,

    inner_rx: mpsc::Receiver<NetworkerToChanneler>,

    // For reporting the closing event
    close_tx: Option<oneshot::Sender<()>>,
    // For observing the closing request
    close_rx: oneshot::Receiver<()>,
}

impl NetworkerReader {
    // Create a new `NetworkerReader`, return a new `NetworkerReader` with its `CloseHandle`.
    pub fn new(
        handle: Handle,
        receiver: mpsc::Receiver<NetworkerToChanneler>,
        neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    ) -> (CloseHandle, NetworkerReader) {
        let (close_handle, (close_tx, close_rx)) = create_close_handle();

        let reader = NetworkerReader {
            handle,
            neighbors,
            close_tx: Some(close_tx),
            close_rx: close_rx,
            inner_rx: receiver,
        };

        (close_handle, reader)
    }

    #[inline]
    fn add_neighbor(&self, info: ChannelerNeighborInfo) {
        let task = self.neighbors.acquire(|mut neighbors| {
            neighbors.insert(
                info.public_key.clone(),
                ChannelerNeighbor {
                    info,
                    num_pending_out_conn: 0,
                    remaining_ticks: 0,
                    channels: Vec::new(),
                });
            Ok((neighbors, ()))
        })
        .map_err(|_: AsyncMutexError<()>| {
            info!("failed to add neighbor");
        });

        self.handle.spawn(task);
    }

    #[inline]
    fn del_neighbor(&self, public_key: PublicKey) {
        let task = self.neighbors.acquire(move |mut neighbors| {
            if neighbors.remove(&public_key).is_some() {
                info!("neighbor {:?} removed", public_key);
            } else {
                info!("nonexistent neighbor: {:?}", public_key);
            }
            Ok((neighbors, ()))
        })
        .map_err(|_: AsyncMutexError<()>| {
            info!("failed to remove neighbor");
        });

        self.handle.spawn(task);
    }

    #[inline]
    fn send_message(&self, index: u32, public_key: PublicKey, content: Bytes) {
        let task = self.neighbors.acquire(move |mut neighbors| {
            match neighbors.get_mut(&public_key) {
                None => {
                    info!(
                        "nonexistent neighbor: {:?}, message would be discard",
                        public_key
                    );
                }
                Some(neighbor) => {
                    if neighbor.channels.is_empty() {
                        debug!("no opened channel, failed to send message");
                    } else {
                        let index = (index as usize) % neighbor.channels.len();
                        let sender = &mut neighbors.channels[index].1;
                        let message = ToChannel::SendMessage(content);

                        // FIXME: Use backpressure strategy here?
                        if sender.try_send(message).is_err() {
                            error!("failed to send message to channel, message will be dropped!");
                        }
                    }
                }
            }

            Ok((neighbors, ()))
        })
        .map_err(|_: AsyncMutexError<()>| {
            info!("failed to pass send message request to channel");
        });

        self.handle.spawn(task);
    }

    #[inline]
    fn set_max_channels(&self, public_key: PublicKey, max_channels: u32) {
        let task = self.neighbors.acquire(move |mut neighbors| {
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

impl Future for NetworkerReader {
    type Item = ();
    type Error = NetworkerReaderError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        trace!("poll - {:?}", ::std::time::Instant::now());

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
                Ok(item) => {
                    match item {
                        Async::NotReady => {
                            return Ok(Async::NotReady);
                        }
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
                                    neighbor_public_key,
                                    channel_index,
                                    content
                                } => {
                                    self.send_message(channel_index, neighbor_public_key, content);
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
