use std::rc::Rc;
use std::cell::RefCell;
use std::collections::LinkedList;

use std::io;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut, BigEndian};
use byteorder::ByteOrder;
use futures::prelude::*;
use futures::sync::mpsc;
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use crypto::identity::{PublicKey, verify_signature};
use security_module::client::{SecurityModuleClient, SecurityModuleClientError};
use timer::messages::FromTimer;
use utils::CloseHandle;

use proto::{Proto, ProtoError};
use proto::channeler::*;
use networker::messages::NetworkerToChanneler;

use self::channel::{ChannelPool, ChannelError as ChannelError};
use self::types::{NeighborTable, ChannelerNeighbor, ChannelerNeighborInfo};
use self::messages::{ChannelEvent, ChannelerToNetworker};

pub mod types;
pub mod config;
pub mod handler;
pub mod channel;
pub mod messages;
pub mod handshake;

//
//use self::config::{RECONNECT_INTERVAL, MAXIMUM_RAND_PADDING_LEN};
//use self::handshake::{HandshakeServer, HandshakeClient, HandshakeError};
//
//pub struct Channeler<RX: Stream, TX: Sink, R: SecureRandom> {
//    /// Executor for spawning tasks.
//    executor: Handle,
//
//    /// Security module client.
//    sm_client: SecurityModuleClient,
//
//    /// Secure random number generator.
//    secure_rng: Rc<R>,
//
//    channels: Rc<RefCell<ChannelPool>>,
//    neighbors: Rc<RefCell<NeighborTable>>,
//
//    handshake_server: HandshakeServer<R>,
//    handshake_client: HandshakeClient<R>,
//
//    timer_receiver: mpsc::Receiver<FromTimer>,
//
//    networker_sender: mpsc::Sender<ChannelerToNetworker>,
//    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
//    networker_buffered: Option<ChannelerToNetworker>,
//
//    external_sender: TX,
//    external_receiver: RX,
//    external_buffered: LinkedList<(SocketAddr, Bytes)>,
//}
//
////// NOTE: When an error returned by Stream::poll, it not a fatal error for futures 0.1
////// check https://github.com/rust-lang-nursery/futures-rs/issues/206 for more details.
//#[derive(Debug)]
//pub enum ChannelerError {
//    IoError(io::Error),
//    ProtoError(ProtoError),
//    ChannelError(ChannelError),
//    HandshakeError(HandshakeError),
//    SecurityModuleClientError(SecurityModuleClientError),
//
//    /// Failed to generate random padding bytes.
//    RandomPaddingGenerationFailed,
//
//    /// Error when polling message from timer.
//    PollTimerError,
//
//    /// **[Fatal Error]** Timer has terminated.
//    TimerTerminated,
//
//    /// Error when polling message from networker.
//    PollNetworkerError,
//    SendToNetworkerError,
//
//    /// **[Fatal Error]** Networker has terminated.
//    NetworkerTerminated,
//
//    PollIncomingError,
//    SendToExternalError,
//    IncomingTerminated,   // fatal error
//
//    PrepareSendError,
//}
//
////struct TimerHandler<SR, TX> {
////    neighbors: Rc<RefCell<NeighborTable>>,
////    secure_rng: Rc<SR>,
////    external_sender: TX,
////    channel_pool: Rc<RefCell<ChannelPool>>,
////    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
////    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
////}
////
////impl<SR, TX> Future for TimerHandler<SR, TX>
////    where SR: SecureRandom,
////          TX: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE>
////{
////    type Item = ();
////    type Error = ChannelerError;
////
////    fn poll(&self) -> Poll<(), ChannelerError> {}
////}
//
////fn handle_reconnect<O, E, R>(
////    neighbors: Rc<RefCell<NeighborTable>>,
////    channel_pool: Rc<RefCell<ChannelPool>>,
////    handshake_client: Rc<RefCell<HandshakeClient<R>>>,
////    external_sender: O,
////)
////    where O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=E>,
////          R: SecureRandom
////{
////    for neighbor in neighbors.borrow_mut().values_mut() {
////        // If current neighbor have disconnected and no relevant handshake
////        if handshake_client.borrow().allow_initiate_handshake(neighbor.remote_public_key()).is_err()
////            || channel_pool.borrow().is_connected(neighbor.remote_public_key()) {
////            continue
////        }
////
////        if neighbor.reconnect_timeout <= 1 {
////            let request_nonce_msg = handshake_client.borrow_mut()
////                .initiate_handshake(neighbor.remote_public_key().clone())
////                .map_err(ChannelerError::HandshakeError)
////                .and_then(move |request_nonce| {
////                    ChannelerMessage::RequestNonce(request_nonce)
////                        .encode()
////                        .map_err(ChannelerError::ProtoError)
////                });
////            let _ = external_sender.try_send();
////        } else {
////            neighbor.reconnect_timeout -= 1;
////        }
////    }
////    let to_reconnect = neighbors.borrow_mut().values_mut()
////        // Find out DISCONNECTED responders.
////        .filter(|neighbor| {
////            handshake_client.borrow().allow_initiate_handshake(neighbor.remote_public_key()).is_ok()
////                && !channel_pool.borrow().is_connected(neighbor.remote_public_key())
////        })
////        // Decrease each `reconnect_timeout` for disconnected responder,
////        // if it hint ZERO, reset `reconnect_timeout` and dump necessary
////        // information to schedule a reconnection.
////        .filter_map(|disconnected_responder| {
////            if disconnected_responder.reconnect_timeout <= 1 {
////                disconnected_responder.reconnect_timeout = RECONNECT_INTERVAL;
////                Some((
////                    disconnected_responder.remote_addr().unwrap(),
////                    disconnected_responder.remote_public_key().clone()
////                ))
////            } else {
////                disconnected_responder.reconnect_timeout -= 1;
////                None
////            }
////        })
////        .collect::<Vec<(SocketAddr, PublicKey)>>();
////
////    for (remote_addr, remote_public_key) in to_reconnect {
////        handshake_client.borrow_mut()
////            .initiate_handshake(remote_public_key)
////            .into_future()
////            .map_err(ChannelerError::HandshakeError)
////            .and_then(move |request_nonce| {
////                ChannelerMessage::RequestNonce(request_nonce)
////                    .encode()
////                    .into_future()
////                    .map_err(ChannelerError::ProtoError)
////            })
////            .and_then(move |encoded_request_nonce| {
////                external_sender
////                    .send((remote_addr, encoded_request_nonce))
////                    .map_err(|_| ChannelerError::SendToExternalError)
////                    .and_then(|_| Ok(()))
////            });
////    }
////}
//
//
////struct TimerHandler<SR, TX> {
////    neighbors: Rc<RefCell<NeighborTable>>,
////    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
////    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
////    external_sender: TX,
////}
//
//fn start_timer_handler_task<SR: SecureRandom + 'static, Si, E>(
//    timer_receiver: mpsc::Receiver<FromTimer>,
//    external_sender: Si,
//    executor: Handle,
//    neighbors: Rc<RefCell<NeighborTable>>,
//    channel_pool: Rc<RefCell<ChannelPool>>,
//    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
//    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
//) -> CloseHandle
//    where Si: Sink<SinkItem=(SocketAddr, Bytes), SinkError=E>
//{
//    unimplemented!()
//}
//
//fn start_incoming_handler_task<SR: SecureRandom + 'static, St, Si, StE, SiE>(
//    external_receiver: St,
//    external_sender: Si,
//    networker_sender: mpsc::Sender<ChannelerToNetworker>,
//    channel_pool: Rc<RefCell<ChannelPool>>,
//    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
//    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
//) -> CloseHandle
//    where St: Stream<Item=(SocketAddr, Bytes), Error=StE>,
//          Si: Sink<SinkItem=(SocketAddr, Bytes), SinkError=SiE>
//{
//    unimplemented!()
//}
//
//fn start_networker_handler_task<SR: SecureRandom + 'static, Si, SiE>(
//    external_sender: Si,
//    neighbors: Rc<RefCell<NeighborTable>>,
//    channel_pool: Rc<RefCell<ChannelPool>>,
//    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
//    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
//) -> CloseHandle
//    where Si: Sink<SinkItem=(SocketAddr, Bytes), SinkError=SiE>
//{
//    unimplemented!()
//}

////fn create_timer_handler_task<SR: SecureRandom + 'static>(
////    neighbors: Rc<RefCell<NeighborTable>>,
////    channel_pool: Rc<RefCell<ChannelPool>>,
////    handshake_client: Rc<RefCell<HandshakeClient<SR>>>,
////    handshake_server: Rc<RefCell<HandshakeServer<SR>>>,
////    timer_receiver: mpsc::Receiver<FromTimer>,
////) -> impl Future<Item = (), Error = ChannelerError> {
////    timer_receiver.for_each(|msg: FromTimer| {
////        handshake_client.borrow_mut().time_tick();
////        handshake_server.borrow_mut().time_tick();
////
////        /* ===== handle_reconnect ===== */
////        let to_reconnect = neighbors.borrow_mut().values_mut()
////            // Find out DISCONNECTED responders.
////            .filter(|neighbor| {
////                handshake_client
////                    .borrow()
////                    .allow_initiate_handshake(neighbor.remote_public_key()).is_ok()
////                    && !channel_pool.borrow().is_connected(neighbor.remote_public_key())
////            })
////            // Decrease each `reconnect_timeout` for disconnected responder,
////            // if it hint ZERO, reset `reconnect_timeout` and dump necessary
////            // information to schedule a reconnection.
////            .filter_map(|disconnected_responder| {
////                if disconnected_responder.reconnect_timeout <= 1 {
////                    disconnected_responder.reconnect_timeout = RECONNECT_INTERVAL;
////                    Some((
////                        disconnected_responder.remote_addr().unwrap(),
////                        disconnected_responder.remote_public_key().clone()
////                    ))
////                } else {
////                    disconnected_responder.reconnect_timeout -= 1;
////                    None
////                }
////            })
////            .collect::<Vec<(SocketAddr, PublicKey)>>();
////    })
////}
//
////fn apply_time_tick_and_find_out_reconnect() -> Vec<(SocketAddr, PublicKey)> {}
//
//
//impl<RX, TX, SR, TE, RE> Channeler<RX, TX, SR>
//    where
//        RX: Stream<Item=(SocketAddr, Bytes), Error=RE>,
//        TX: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
//        SR: SecureRandom,
//        RE: Into<ChannelerError>,
//        TE: Into<ChannelerError>,
//{
//    /// Try to poll next timer message then process periodic tasks if timer tick fired.
//    ///
//    /// # Periodic Tasks
//    ///
//    /// - Notify `Handshaker` the timer tick fired by calling `Handshaker::time_tick`.
//    /// - Start reconnect tasks if a neighbor disconnected and its `reconnect_timeout` hint ZERO.
//    /// - For each established channel, send a keepalive message to keep NAT session open if needed.
//    ///
//    /// # Returns
//    ///
//    /// If there no message from `TimerModule`, return `Async::NotReady`. Otherwise, MUST return a
//    /// Ok(Async::Ready()), error occur in processing periodic tasks should be processed in place?
//    fn process_timer_next(&mut self) -> Poll<(), ChannelerError> {
//        let polled_timer_msg = self.timer_receiver.poll()
//            .map_err(|_| ChannelerError::PollTimerError);
//
//        match try_ready!(polled_timer_msg) {
//            None => Err(ChannelerError::TimerTerminated),
//            Some(FromTimer::TimeTick) => {
//                self.handshake_server.time_tick();
//                self.handshake_client.time_tick();
//
//                self.handle_reconnect();
//                self.handle_heartbeat();
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    /// Handles the reconnect tasks.
//    fn handle_reconnect(&mut self) {
//        let to_reconnect = self.neighbors.borrow_mut().values_mut()
//            // Find out DISCONNECTED responders.
//            .filter(|neighbor| {
//                self.handshake_client.allow_initiate_handshake(neighbor.remote_public_key()).is_ok()
//                    && !self.channels.borrow().is_connected(neighbor.remote_public_key())
//            })
//            // Decrease each `reconnect_timeout` for disconnected responder,
//            // if it hint ZERO, reset `reconnect_timeout` and dump necessary
//            // information to schedule a reconnection.
//            .filter_map(|disconnected_responder| {
//                if disconnected_responder.reconnect_timeout <= 1 {
//                    disconnected_responder.reconnect_timeout = RECONNECT_INTERVAL;
//                    Some((
//                        disconnected_responder.remote_addr().unwrap(),
//                        disconnected_responder.remote_public_key().clone()
//                    ))
//                } else {
//                    disconnected_responder.reconnect_timeout -= 1;
//                    None
//                }
//            })
//            .collect::<Vec<(SocketAddr, PublicKey)>>();
//
//        for (remote_addr, remote_public_key) in to_reconnect {
//            // Will create a Future for each request nonce task
//            self.start_new_handshake(remote_addr, remote_public_key);
//        }
//    }
//
//    /// Handles the heartbeat task.
//    ///
//    /// The purpose of heartbeat is to keep NAT session open. To archive this goal,
//    /// we send heartbeat packet(aka. keepalive message) periodically:
//    ///
//    /// - First, we notify `ChannelPool` the timer tick fired and get public keys
//    ///   we need to send keepalive message to.
//    /// - Second, we construct a keepalive message and push it into external buffer.
//    ///
//    /// **NOTE:** All keepalive messages will be pushed in the buffer deque, which
//    /// intend to be sent at the beginning at the next poll-loop. Also, the errors
//    /// occur in encrypt and decode are be ignored.
//    fn handle_heartbeat(&mut self) {
//        let keepalive_fired = self.channels.borrow_mut().time_tick();
//
//        for remote_public_key in keepalive_fired {
//            match self.prepare_channel_msg(&remote_public_key, PlainContent::KeepAlive) {
//                Ok(item) => self.external_buffered.push_back(item),
//                Err(e) => {
//                    error!("failed to prepare keepalive msg for {:?}: {:?}", remote_public_key, e)
//                }
//            }
//        }
//    }
//
//    fn process_networker_next(&mut self) -> Poll<(), ChannelerError> {
//        let polled_networker_msg = self.networker_receiver.poll()
//            .map_err(|_| ChannelerError::PollNetworkerError);
//
//        match try_ready!(polled_networker_msg) {
//            None => Err(ChannelerError::NetworkerTerminated),
//            Some(networker_message) => {
//                match networker_message {
//                    NetworkerToChanneler::AddNeighbor { info } => {
//                        self.add_neighbor(info);
//                    }
//                    NetworkerToChanneler::RemoveNeighbor {
//                        neighbor_public_key
//                    } => {
//                        self.remove_neighbor(neighbor_public_key);
//                    }
//                    NetworkerToChanneler::SendChannelMessage {
//                        neighbor_public_key,
//                        content,
//                    } => {
//                        return self.do_send_application_msg(
//                            neighbor_public_key,
//                            content,
//                        );
//                    }
//                }
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    fn process_incoming_next(&mut self) -> Poll<(), ChannelerError> {
//        let polled_incoming_msg = self.external_receiver.poll()
//            .map_err(|_| ChannelerError::PollIncomingError);
//
//        match try_ready!(polled_incoming_msg) {
//            None => Err(ChannelerError::IncomingTerminated),
//            Some((remote_addr, raw_message)) => {
//                let channeler_message = ChannelerMessage::decode(&raw_message)
//                    .map_err(ChannelerError::ProtoError)?;
//
//                match channeler_message {
//                    ChannelerMessage::Encrypted(encrypted) => {
//                        let opt_message = self.channels.borrow_mut()
//                            .decrypt_msg(encrypted)
//                            .map_err(ChannelerError::ChannelError)?;
//
//                        if let (remote_public_key, Some(message)) = opt_message {
//                            let message_to_networker = ChannelerToNetworker {
//                                remote_public_key,
//                                event: ChannelEvent::Message(message),
//                            };
//                            return self.start_send_networker(message_to_networker);
//                        }
//                    }
//                    ChannelerMessage::RequestNonce(request_nonce) => {
//                        self.process_request_nonce(remote_addr, request_nonce);
//                    }
//                    ChannelerMessage::ResponseNonce(respond_nonce) => {
//                        self.process_respond_nonce(remote_addr, respond_nonce);
//                    }
//                    ChannelerMessage::ExchangeActive(exchange_active) => {
//                        self.process_exchange_active(remote_addr, exchange_active);
//                    }
//                    ChannelerMessage::ExchangePassive(exchange_passive) => {
//                        self.process_exchange_passive(remote_addr, exchange_passive);
//                    }
//                    ChannelerMessage::ChannelReady(channel_ready) => {
//                        self.process_channel_ready(remote_addr, channel_ready);
//                    }
//                    ChannelerMessage::UnknownChannel(unknown_channel) => {
//                        self.process_unknown_channel(unknown_channel);
//                    }
//                }
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    fn add_neighbor(&mut self, info: ChannelerNeighborInfo) {
//        let neighbor_public_key = info.public_key.clone();
//
//        if !self.neighbors.borrow().contains_key(&neighbor_public_key) {
//            let new_neighbor = ChannelerNeighbor::new(info);
//            self.neighbors.borrow_mut().insert(neighbor_public_key, new_neighbor);
//        } else {
//            info!("neighbor: {:?} exist, do nothing", info.public_key);
//        }
//    }
//
//    /// Remove specified neighbor from channeler
//    ///
//    /// For removing a channeler neighbor, we **SHOULD** have the following done:
//    ///
//    /// - Remove the neighbor's information from neighbors table.
//    /// - Remove the sending end from the sending ends table.
//    /// - Remove all the relevant receiving ends from the receiving ends table.
//    fn remove_neighbor(&mut self, public_key: PublicKey) {
//        trace!("request to remove neighbor: {:?}", public_key);
//
//        self.neighbors.borrow_mut().remove(&public_key);
//        self.channels.borrow_mut().remove_channel(&public_key);
//        self.handshake_server.remove_session_by_public_key(&public_key);
//        self.handshake_client.remove_session_by_public_key(&public_key);
//    }
//
//    fn prepare_channel_msg(
//        &self,
//        remote_public_key: &PublicKey,
//        content: PlainContent,
//    ) -> Result<(SocketAddr, Bytes), ChannelerError> {
//        gen_random_bytes(MAXIMUM_RAND_PADDING_LEN, &*self.secure_rng)
//            .map_err(|_| ChannelerError::RandomPaddingGenerationFailed)
//            .and_then(|rand_padding| {
//                let plain = Plain { rand_padding, content };
//                self.channels.borrow_mut()
//                    .encrypt_msg(remote_public_key, plain)
//                    .map_err(ChannelerError::ChannelError)
//            })
//    }
//
//    fn do_send_application_msg(
//        &mut self,
//        remote_public_key: PublicKey,
//        content: Bytes,
//    ) -> Poll<(), ChannelerError> {
//        let item = self.prepare_channel_msg(
//            &remote_public_key,
//            PlainContent::Application(content),
//        )?;
//
//        self.start_send_external(item)
//    }
//
//    fn start_new_handshake(&mut self, remote_addr: SocketAddr, remote_public_key: PublicKey) {
//        let external_sender = self.external_sender.clone();
//
//        let start_new_handshake_task = self.handshake_client
//            .initiate_handshake(remote_public_key)
//            .into_future()
//            .map_err(ChannelerError::HandshakeError)
//            .and_then(move |request_nonce| {
//                ChannelerMessage::RequestNonce(request_nonce)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::ProtoError)
//            })
//            .and_then(move |encoded_request_nonce| {
//                external_sender
//                    .send((remote_addr, encoded_request_nonce))
//                    .map_err(|_| ChannelerError::SendToExternalError)
//                    .and_then(|_| Ok(()))
//            });
//
//        self.executor.spawn(start_new_handshake_task.map_err(|e| {
//            info!("failed to start new handshake: {:?}", e);
//        }));
//    }
//
//    fn process_request_nonce(&mut self, remote_addr: SocketAddr, request_nonce: RequestNonce) {
//        let sm_client = self.sm_client.clone();
//        let external_sender = self.external_sender.clone();
//
//        let process_request_nonce_task = self.handshake_server
//            .handle_request_nonce(request_nonce)
//            .into_future()
//            .map_err(ChannelerError::HandshakeError)
//            .and_then(move |mut respond_nonce| {
//                sm_client.request_signature(respond_nonce.as_bytes().to_vec())
//                    .map_err(ChannelerError::SecurityModuleClientError)
//                    .and_then(move |signature| {
//                        respond_nonce.signature = signature;
//                        Ok(respond_nonce)
//                    })
//            })
//            .and_then(|respond_nonce| {
//                ChannelerMessage::ResponseNonce(respond_nonce)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::ProtoError)
//            })
//            .and_then(move |encoded_response_nonce| {
//                external_sender
//                    .send((remote_addr, encoded_response_nonce))
//                    .map_err(|_| ChannelerError::SendToExternalError)
//                    .and_then(|_| Ok(()))
//            });
//
//        self.executor.spawn(process_request_nonce_task.map_err(|e| {
//            info!("failed to process request nonce message: {:?}", e);
//        }));
//    }
//
//    fn process_respond_nonce(&mut self, remote_addr: SocketAddr, respond_nonce: ResponseNonce) {
//        let sm_client = self.sm_client.clone();
//        let external_sender = self.external_sender.clone();
//
//        let process_respond_nonce_task = self.handshake_client
//            .handle_response_nonce(respond_nonce)
//            .into_future()
//            .map_err(ChannelerError::HandshakeError)
//            .and_then(move |mut exchange_active| {
//                sm_client.request_signature(exchange_active.as_bytes().to_vec())
//                    .map_err(ChannelerError::SecurityModuleClientError)
//                    .and_then(move |signature| {
//                        exchange_active.signature = signature;
//                        Ok(exchange_active)
//                    })
//            })
//            .and_then(|exchange_active| {
//                ChannelerMessage::ExchangeActive(exchange_active)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::ProtoError)
//            })
//            .and_then(move |encoded_exchange_active| {
//                external_sender
//                    .send((remote_addr, encoded_exchange_active))
//                    .map_err(|_| ChannelerError::SendToExternalError)
//                    .and_then(|_| Ok(()))
//            });
//
//        self.executor.spawn(process_respond_nonce_task.map_err(|e| {
//            info!("failed to process respond nonce message: {:?}", e);
//        }));
//    }
//
//    fn process_exchange_active(&mut self, remote_addr: SocketAddr, exchange_active: ExchangeActive) {
//        let sm_client = self.sm_client.clone();
//        let external_sender = self.external_sender.clone();
//
//        let process_exchange_active_task = self.handshake_server
//            .handle_exchange_active(exchange_active)
//            .into_future()
//            .map_err(ChannelerError::HandshakeError)
//            .and_then(move |mut exchange_passive| {
//                sm_client.request_signature(exchange_passive.as_bytes().to_vec())
//                    .map_err(ChannelerError::SecurityModuleClientError)
//                    .and_then(move |signature| {
//                        exchange_passive.signature = signature;
//                        Ok(exchange_passive)
//                    })
//            })
//            .and_then(|exchange_passive| {
//                ChannelerMessage::ExchangePassive(exchange_passive)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::ProtoError)
//            })
//            .and_then(move |encoded_exchange_passive| {
//                external_sender
//                    .send((remote_addr, encoded_exchange_passive))
//                    .map_err(|_| ChannelerError::SendToExternalError)
//                    .and_then(move |_| Ok(()))
//            });
//
//        self.executor.spawn(process_exchange_active_task.map_err(|e| {
//            info!("failed to process exchange active message: {:?}", e);
//        }));
//    }
//
//    fn process_exchange_passive(&mut self, remote_addr: SocketAddr, exchange_passive: ExchangePassive) {
//        let sm_client = self.sm_client.clone();
//        let channel_pool = Rc::clone(&self.channels);
//        let external_sender = self.external_sender.clone();
//
//        let process_exchange_passive_task = self.handshake_client
//            .handle_exchange_passive(exchange_passive)
//            .into_future()
//            .map_err(ChannelerError::HandshakeError)
//            .and_then(move |(channel_metadata, mut channel_ready)| {
//                sm_client.request_signature(channel_ready.as_bytes().to_vec())
//                    .map_err(ChannelerError::SecurityModuleClientError)
//                    .and_then(move |signature| {
//                        channel_ready.signature = signature;
//                        Ok((channel_metadata, channel_ready))
//                    })
//            })
//            .and_then(|(channel_metadata, channel_ready)| {
//                ChannelerMessage::ChannelReady(channel_ready)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::ProtoError)
//                    .and_then(move |encoded_channel_ready| {
//                        Ok((encoded_channel_ready, channel_metadata))
//                    })
//            })
//            .and_then(move |(encoded_channel_ready, channel_metadata)| {
//                external_sender
//                    .send((remote_addr, encoded_channel_ready))
//                    .map_err(|_| ChannelerError::SendToExternalError)
//                    .and_then(move |_| Ok(channel_metadata))
//            })
//            .and_then(move |channel_metadata| {
//                channel_pool.borrow_mut().insert_channel(remote_addr, channel_metadata);
//                Ok(())
//            });
//
//        self.executor.spawn(process_exchange_passive_task.map_err(|e| {
//            info!("failed to process exchange passive message: {:?}", e);
//        }));
//    }
//
//    fn process_channel_ready(&mut self, remote_addr: SocketAddr, channel_ready: ChannelReady) {
//        let channel_pool = Rc::clone(&self.channels);
//
//        match self.handshake_server.handle_channel_ready(channel_ready) {
//            Ok(channel_metadata) => {
//                channel_pool.borrow_mut().insert_channel(remote_addr, channel_metadata);
//            }
//            Err(e) => {
//                error!("failed to process channel ready message: {:?}", e);
//            }
//        }
//    }
//
//    fn process_unknown_channel(&mut self, unknown_channel: UnknownChannel) {
//        let mut borrowed_mut_channels = self.channels.borrow_mut();
//
//        match borrowed_mut_channels.get_tx_public_key(&unknown_channel.channel_id) {
//            None => return,
//            Some(remote_public_key_ref) => {
//                if !verify_signature(
//                    &unknown_channel.as_bytes(),
//                    remote_public_key_ref,
//                    &unknown_channel.signature,
//                ) {
//                    return;
//                }
//            }
//        }
//
//        borrowed_mut_channels.remove_channel_tx(&unknown_channel.channel_id);
//        // TODO: Attempt to setup a new connection
//    }
//
//    fn start_send_networker(&mut self, item: ChannelerToNetworker) -> Poll<(), ChannelerError> {
//        debug_assert!(self.networker_buffered.is_none());
//
//        let start_send_result = self.networker_sender.start_send(item)
//            .map_err(|_| ChannelerError::SendToNetworkerError);
//
//        if let AsyncSink::NotReady(item) = start_send_result? {
//            self.networker_buffered = Some(item);
//            return Ok(Async::NotReady);
//        }
//
//        Ok(Async::Ready(()))
//    }
//
//    fn start_send_external(&mut self, item: (SocketAddr, Bytes)) -> Poll<(), ChannelerError> {
//        let start_send_result = self.external_sender.start_send(item)
//            .map_err(|_| ChannelerError::SendToExternalError);
//
//        if let AsyncSink::NotReady(item) = start_send_result? {
//            self.external_buffered.push_front(item);
//            return Ok(Async::NotReady);
//        }
//
//        Ok(Async::Ready(()))
//    }
//}
//
//impl<I, O, SR, TE, RE> Future for Channeler<I, O, SR>
//    where
//        I: Stream<Item=(SocketAddr, Bytes), Error=RE>,
//        O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
//        SR: SecureRandom,
//        TE: Into<ChannelerError>,
//        RE: Into<ChannelerError>,
//{
//    type Item = ();
//    type Error = ChannelerError;
//
//    fn poll(&mut self) -> Poll<(), ChannelerError> {
//        loop {
//            while let Some(external_msg) = self.external_buffered.pop_front() {
//                try_ready!(self.start_send_external(external_msg));
//            }
//
//            if let Some(networker_msg) = self.networker_buffered.take() {
//                try_ready!(self.start_send_networker(networker_msg));
//            }
//            if let Async::NotReady = self.process_timer_next()? {
//                if let Async::NotReady = self.process_networker_next()? {
//                    if let Async::NotReady = self.process_incoming_next()? {
//                        return Ok(Async::NotReady);
//                    }
//                }
//            }
//        }
//    }
//}

#[inline]
pub fn gen_random_bytes(max_len: usize, rng: &SecureRandom) -> Result<Bytes, ()> {
    if (u16::max_value() as usize + 1) % max_len != 0 {
        return Err(());
    }

    let mut len_bytes = [0x00; 2];
    rng.fill(&mut len_bytes[..]).map_err(|_| ())?;
    let len = BigEndian::read_u16(&len_bytes[..]) as usize % max_len + 1;

    let mut bytes = BytesMut::from(vec![0x00; len]);
    rng.fill(&mut bytes[..len]).map_err(|_| ())?;

    Ok(bytes.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn test_gen_random_bytes() {
        let fixed = FixedByteRandom { byte: 0x01 };
        let bytes = gen_random_bytes(32, &fixed).unwrap();

        assert_eq!(bytes.len(), 2);
        assert!(bytes.iter().all(|x| *x == 0x01));
    }
}
