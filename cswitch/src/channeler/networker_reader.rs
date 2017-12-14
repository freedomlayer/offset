use std::collections::HashMap;

use futures::{Future, Stream};
use futures::sync::mpsc;
use futures_mutex::FutMutex;
use tokio_core::reactor::Handle;

use inner_messages::{NetworkerToChanneler};
use crypto::identity::PublicKey;
use super::{ChannelerNeighbor, ToChannel};

pub enum NetworkerReaderError {
    MessageReceiveFailed,
    FutMutex,
}

pub fn create_networker_reader_future(
    handle: Handle,
    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
    neighbors: FutMutex<HashMap<PublicKey, ChannelerNeighbor>>
) -> impl Future<Item=(), Error=NetworkerReaderError> {
    networker_receiver
        .map_err(|_| NetworkerReaderError::MessageReceiveFailed)
        .map(move |message| {
            let neighbors = neighbors.clone();

            (neighbors, message)
        })
        .for_each(move |(neighbors, message)| {
            match message {
                NetworkerToChanneler::AddNeighbor { neighbor_info } => {
                    let add_neighbor_task = neighbors.lock()
                        .map_err(|_| NetworkerReaderError::FutMutex)
                        .and_then(move |mut neighbors| {
                            neighbors.insert(neighbor_info.neighbor_address.neighbor_public_key.clone(),
                                             ChannelerNeighbor {
                                                 info: neighbor_info,
                                                 num_pending_out_conn: 0,
                                                 remaining_retry_ticks: 0,
                                                 channels: Vec::new(),
                                             });
                            Ok(())
                        });

                    handle.spawn(add_neighbor_task.map_err(|_| ()));
                }
                NetworkerToChanneler::RemoveNeighbor { neighbor_public_key } => {
                    let del_neighbor_task = neighbors.lock()
                        .map_err(|_| NetworkerReaderError::FutMutex)
                        .and_then(move |mut neighbors| {
                        if neighbors.remove(&neighbor_public_key).is_some() {
                            trace!("finished to remove neighbor");
                        } else {
                            warn!("attempt to remove a nonexistent neighbor: {:?}", neighbor_public_key);
                        }
                        Ok(())
                    });

                    handle.spawn(del_neighbor_task.map_err(|_| ()));
                }
                NetworkerToChanneler::SendChannelMessage { token, neighbor_public_key, content } => {
                    let send_message_task = neighbors.lock()
                        .map_err(|_| NetworkerReaderError::FutMutex)
                        .and_then(move |mut neighbors| {
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

                        Ok(())
                    });

                    handle.spawn(send_message_task.map_err(|_| ()));
                }
                NetworkerToChanneler::SetMaxChannels { neighbor_public_key, max_channels } => {
                    let set_max_channel_task = neighbors.lock()
                        .map_err(|_| NetworkerReaderError::FutMutex)
                        .and_then(move |mut neighbors| {
                        match neighbors.get_mut(&neighbor_public_key) {
                            None => warn!("attempt to change max_channels for a nonexistent neighbor"),
                            Some(neighbors) => {
                                neighbors.info.max_channels = max_channels;
                            }
                        }

                        Ok(())
                    });

                    handle.spawn(set_max_channel_task.map_err(|_| ()));
                }
            }

            Ok(())
        })
}
