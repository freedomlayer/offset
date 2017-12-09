use std::collections::HashMap;

use futures::{Future, Stream};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use std::cell::RefCell;
use std::rc::Rc;

use inner_messages::{NetworkerToChanneler};
use security_module::security_module_client::SecurityModuleClient;
use crypto::rand_values::RandValuesStore;
use crypto::identity::{PublicKey};
use super::ChannelerNeighbor;

pub enum NetworkerReaderError {
    MessageReceiveFailed,
}

pub fn networker_reader_future<R>(handle: Handle,
                                  networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
                                  security_module_client: SecurityModuleClient,
                                  crypt_rng: Rc<R>,
                                  rand_values_store: Rc<RefCell<RandValuesStore>>,
                                  neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>)
                                  -> impl Future<Item=(), Error=NetworkerReaderError> {
    networker_receiver
        .map_err(|_| NetworkerReaderError::MessageReceiveFailed)
        .for_each(|message| {
            match message {
                NetworkerToChanneler::AddNeighborRelation { neighbor_info } => {

                }
                NetworkerToChanneler::RemoveNeighborRelation { neighbor_public_key } => {

                }
                NetworkerToChanneler::SendChannelMessage { neighbor_public_key, message_content } => {

                }
                NetworkerToChanneler::SetMaxChannels { neighbor_public_key, max_channels } => {

                }
                NetworkerToChanneler::SetServerType(_) => {
                    unimplemented!()
                }
            }

            Ok(())
        })
}

//fn handle_networker_message(&mut self, message: NetworkerToChanneler) ->
//    StartSend<NetworkerToChanneler, ChannelerError> {
//
//    match message {
//        NetworkerToChanneler::SendChannelMessage {
//            neighbor_public_key, message_content } => {
//            // TODO: Attempt to send a message to some TCP connections leading to
//            // the requested neighbor. The chosen Sink could be not ready.
//            Ok(AsyncSink::Ready)
//        },
//        NetworkerToChanneler::AddNeighborRelation { neighbor_info } => {
//            let neighbor_public_key = neighbor_info.neighbor_public_key.clone();
//            if self.neighbors.contains_key(&neighbor_public_key) {
//                warn!("Neighbor with public key {:?} already exists",
//                      neighbor_public_key);
//            }
//            self.neighbors.insert(neighbor_public_key, ChannelerNeighbor {
//                info: neighbor_info,
//                last_remote_rand_value: None,
//                channels: Vec::new(),
//                pending_channels: Vec::new(),
//                pending_out_conn: None,
//                ticks_to_next_conn_attempt: CONN_ATTEMPT_TICKS,
//            });
//            Ok(AsyncSink::Ready)
//        },
//        NetworkerToChanneler::RemoveNeighborRelation { neighbor_public_key } => {
//            match self.neighbors.remove(&neighbor_public_key) {
//                None => warn!("Attempt to remove a nonexistent neighbor \
//                    relation with public key {:?}", neighbor_public_key),
//                _ => {},
//            };
//            // TODO: Possibly close all connections here.
//
//            Ok(AsyncSink::Ready)
//        },
//        NetworkerToChanneler::SetMaxChannels
//            { neighbor_public_key, max_channels } => {
//            match self.neighbors.get_mut(&neighbor_public_key) {
//                None => warn!("Attempt to change max_channels for a \
//                    nonexistent neighbor relation with public key {:?}",
//                    neighbor_public_key),
//                Some(neighbor) => {
//                    neighbor.info.max_channels = max_channels;
//                },
//            };
//            Ok(AsyncSink::Ready)
//        },
//        NetworkerToChanneler::SetServerType(server_type) => {
//            self.server_type = server_type;
//            Ok(AsyncSink::Ready)
//        },
//    }
//}


