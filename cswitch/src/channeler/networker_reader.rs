use std::collections::HashMap;

use futures::{Future, Stream};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use async_mutex::{AsyncMutex, AsyncMutexError};
use inner_messages::{NetworkerToChanneler};
use security_module::security_module_client::SecurityModuleClient;
use crypto::rand_values::RandValuesStore;
use crypto::identity::PublicKey;
use super::{ChannelerNeighbor, ToChannel};

pub enum NetworkerReaderError {
    MessageReceiveFailed,
}

pub fn create_networker_reader_future(handle: Handle,
                                      networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
                                      security_module_client: SecurityModuleClient,
                                      neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>)
    -> impl Future<Item=(), Error=NetworkerReaderError> {
    networker_receiver
        .map_err(|_| NetworkerReaderError::MessageReceiveFailed)
        .for_each(move |message| {
            match message {
                NetworkerToChanneler::AddNeighbor { neighbor_info } => {
                    let add_neighbor_task = neighbors.acquire(move |mut neighbors| {
                        neighbors.insert(neighbor_info.neighbor_address.neighbor_public_key.clone(),
                                         ChannelerNeighbor {
                                             info: neighbor_info,
                                             num_pending_out_conn: 0,
                                             ticks_to_next_conn_attempt: 0,
                                             channels: Vec::new(),
                                         });
                        Ok((neighbors, ()))
                    }).map_err(|_: AsyncMutexError<()>| ());

                    handle.spawn(add_neighbor_task);
                }
                NetworkerToChanneler::RemoveNeighbor { neighbor_public_key } => {
                    let del_neighbor_task = neighbors.acquire(move |mut neighbors| {
                        if neighbors.remove(&neighbor_public_key).is_some() {
                            trace!("finished to remove neighbor");
                        } else {
                            warn!("attempt to remove a nonexistent neighbor: {:?}", neighbor_public_key);
                        }
                        Ok((neighbors, ()))
                    }).map_err(|_: AsyncMutexError<()>| ());

                    handle.spawn(del_neighbor_task);
                }
                NetworkerToChanneler::SendChannelMessage { token, neighbor_public_key, content } => {
                    neighbors.acquire(move |mut neighbors| {
                        match neighbors.get_mut(&neighbor_public_key) {
                            None => warn!("attempt to send message to a nonexistent neighbor"),
                            Some(neighbors) => {
                                let channel_idx = (token as usize) % neighbors.channels.len();

                                let channel_sender = &mut neighbors.channels[channel_idx].1;
                                let msg_to_channel = ToChannel::SendMessage(content);

                                // TODO: Use backpressure strategy here?
                                match channel_sender.try_send(msg_to_channel) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        error!("failed to send message to channel");
                                    }
                                }
                            }
                        }

                        Ok((neighbors, ()))
                    }).map_err(|_: AsyncMutexError<()>| ());
                }
                NetworkerToChanneler::SetMaxChannels { neighbor_public_key, max_channels } => {
                    let set_max_channel_task = neighbors.acquire(move |mut neighbors| {
                        match neighbors.get_mut(&neighbor_public_key) {
                            None => warn!("attempt to change max_channels for a nonexistent neighbor"),
                            Some(neighbors) => {
                                neighbors.info.max_channels = max_channels;
                            }
                        }

                        Ok((neighbors, ()))
                    }).map_err(|_: AsyncMutexError<()>| ());

                    handle.spawn(set_max_channel_task);
                }
            }

            Ok(())
        })
}
