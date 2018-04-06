use std::rc::Rc;
use std::cell::RefCell;

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut, Buf, BufMut, BigEndian};
use byteorder::{ByteOrder, LittleEndian};
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

// FIXME
use channeler::types::ChannelerNeighborInfo;
use crypto::identity::PublicKey;
use security_module::client::SecurityModuleClient;
use timer::messages::FromTimer;

use proto::{Proto, ProtoError};
use networker::messages::NetworkerToChanneler;

use proto::channeler::{ChannelId, CHANNEL_ID_LEN, ChannelerMessage, RequestNonce,
    RespondNonce, ExchangeActive, ExchangePassive, ChannelReady, PlainContent, Plain};

use self::channel::{ChannelPool, ChannelPoolError, ChannelPoolConfig};
use self::handshake::HandshakeStateMachine;

pub mod types;
pub mod channel;
pub mod messages;
pub mod handshake;
//
//// TODO: Introduce `ChannelerConfig`
//const RETRY_TICKS: usize = 100;
//const MAX_RAND_PADDING_LEN: usize = 32;
//
//#[derive(Clone)]
//pub struct NeighborInfo {
//    pub public_key: PublicKey,
//    pub socket_addr: Option<SocketAddr>,
//    pub retry_ticks: usize,
//}
//
//type NeighborsTable = HashMap<PublicKey, NeighborInfo>;
//
//pub struct Channeler<I: Stream, O: Sink, SR: SecureRandom> {
//    handle: Handle,
//
//    /// Security module client.
//    sm_client: SecurityModuleClient,
//
//    /// Secure random number generator
//    secure_rng: Rc<SR>,
//
//    channel_pool: Rc<RefCell<ChannelPool>>,
//    neighbors: Rc<RefCell<NeighborsTable>>,
//
//    handshake_manager: HandshakeStateMachine<SR>,
//
//    timer_receiver: mpsc::Receiver<FromTimer>,
//
//    networker_sender: mpsc::Sender<ChannelerToNetworker>,
//    networker_buffered: Option<ChannelerToNetworker>,
//
//    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,
//
//    outgoing_sender: O,
//    outgoing_buffered: Option<(SocketAddr, Bytes)>,
//
//    incoming_receiver: I,
//}
//
//// NOTE: When an error returned by Stream::poll, it not a fatal error for futures 0.1
//// check https://github.com/rust-lang-nursery/futures-rs/issues/206 for more details.
//#[derive(Debug)]
//pub enum ChannelerError {
//    Io(io::Error),
//
//    Proto(ProtoError),
//
//    PollTimerError,
//    TimerTerminated,      // fatal error
//
//    PollNetworkerError,
//    SendToNetworkerError,
//    // fatal error
//    NetworkerTerminated,  // fatal error
//
//    PollIncomingError,
//    SendOutgoingError,
//    // fatal error
//    IncomingTerminated,   // fatal error
//
//    SecureRandomError,
//    PrepareSendError,
//
//    ChannelPool(ChannelPoolError),
//
//    SendToHandshakeManagerError, // fatal error
//
//    HandshakeManagerError,
//}
//
//impl From<ChannelPoolError> for ChannelerError {
//    fn from(e: ChannelPoolError) -> ChannelerError {
//        ChannelerError::ChannelPool(e)
//    }
//}
//
//impl<I, O, SR, TE, RE> Channeler<I, O, SR>
//    where
//        I: Stream<Item=(SocketAddr, Bytes), Error=RE>,
//        O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
//        SR: SecureRandom,
//        RE: Into<ChannelerError>,
//        TE: Into<ChannelerError>,
//{
//    fn process_timer_next(&mut self) -> Poll<(), ChannelerError> {
//        let poll_result = self.timer_receiver.poll().map_err(|_| ChannelerError::PollTimerError);
//
//        match try_ready!(poll_result) {
//            None => Err(ChannelerError::TimerTerminated),
//            Some(FromTimer::TimeTick) => {
//                // Decrease the retry ticks of all neighbor, initiate a new handshake
//                // if the counter reach ZERO
//                for neighbor in self.neighbors.borrow_mut().values_mut() {
//                    if let Some(remote_addr) = neighbor.socket_addr {
//                        if neighbor.retry_ticks <= 1 {
//                            neighbor.retry_ticks = RETRY_TICKS;
//                            self.process_new_handshake(remote_addr, neighbor.public_key.clone());
//                        } else {
//                            neighbor.retry_ticks -= 1;
//                        }
//                    }
//                }
//
//                // TODO: Notify the handshake manager
//
//                // TODO: Notify the channel pool and sent the keepalive message, if any.
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    fn process_networker_next(&mut self) -> Poll<(), ChannelerError> {
//        let poll_result = self.networker_receiver.poll()
//            .map_err(|_| ChannelerError::PollNetworkerError);
//
//        match try_ready!(poll_result) {
//            None => Err(ChannelerError::NetworkerTerminated),
//            Some(networker_message) => {
//                match networker_message {
//                    NetworkerToChanneler::AddNeighbor { info } => {
//                        self.add_neighbor(info);
//                    },
//                    NetworkerToChanneler::RemoveNeighbor { public_key } => {
//                        self.remove_neighbor(public_key);
//                    },
//                    NetworkerToChanneler::SendChannelMessage {
//                        neighbor_public_key,
//                        content,
//                    } => {
//                        return self.do_send_channel_message(neighbor_public_key, content);
//                    }
//                }
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    fn process_incoming_next(&mut self) -> Poll<(), ChannelerError> {
//        let poll_result = self.incoming_receiver.poll()
//            .map_err(|_| ChannelerError::PollIncomingError);
//
//        match try_ready!(poll_result) {
//            None => Err(ChannelerError::IncomingTerminated),
//            Some((remote_addr, plain)) => {
//                let channeler_message = ChannelerMessage::decode(&plain)
//                    .map_err(ChannelerError::Proto)?;
//
//                match channeler_message {
//                    ChannelerMessage::Encrypted(encrypted) => {
//                        let opt_message =
//                            self.channel_pool.borrow_mut().decrypt_incoming(encrypted)?;
//
//                        if let Some((pk, message)) = opt_message {
//                            let message_to_networker = ChannelerToNetworker {
//                                remote_public_key: pk,
//                                event: ChannelEvent::Message(message),
//                            };
//                            return self.start_send_networker(message_to_networker);
//                        }
//                    }
//                    ChannelerMessage::InitChannel(init_channel) => {
//                        self.process_init_channel(remote_addr, init_channel);
//                    }
//                    ChannelerMessage::ChannelReady(channel_ready) => {
//                        self.process_channel_ready(remote_addr, channel_ready);
//                    }
//                    ChannelerMessage::UnknownChannel(_unknown_channel) => {
//                        unimplemented!()
//                    }
//                    ChannelerMessage::ExchangeActive(exchange_active) => {
//                        self.process_exchange_active(remote_addr, exchange_active);
//                    }
//                    ChannelerMessage::ExchangePassive(exchange_passive) => {
//                        self.process_exchange_passive(remote_addr, exchange_passive);
//                    }
//                }
//
//                Ok(Async::Ready(()))
//            }
//        }
//    }
//
//    // ===========================================================================================
//
//    fn add_neighbor(&mut self, info: ChannelerNeighborInfo) {
//        trace!("request to add neighbor: {:?}", info.public_key);
//
//        if !self.neighbors.borrow().contains_key(&info.public_key) {
//            let new_neighbor = NeighborInfo {
//                public_key: info.public_key.clone(),
//                socket_addr: info.socket_addr,
//                retry_ticks: 0,
//            };
//            self.neighbors.borrow_mut().insert(info.public_key, new_neighbor);
//        } else {
//            info!("neighbor: {:?} exist, do nothing", info.public_key);
//        }
//    }
//
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
//        self.channel_pool.borrow_mut().remove(&public_key);
//        self.neighbors.borrow_mut().remove(&public_key);
//    }
//
//    fn do_send_channel_message(&mut self, public_key: PublicKey, content: Bytes) -> Poll<(), ChannelerError> {
//        let rand_padding = gen_random_bytes(&*self.secure_rng, MAX_RAND_PADDING_LEN)
//            .map_err(|_| ChannelerError::SecureRandomError)?;
//
//        let plain = Plain {
//            rand_padding,
//            content: PlainContent::User(content)
//        };
//
//        let item = self.channel_pool.borrow_mut().encrypt_outgoing(&public_key, plain)?;
//
//        self.start_send_outgoing(item)
//    }
//
//    fn process_new_handshake(&self, remote_addr: SocketAddr, neighbor_public_key: PublicKey) {
//        let sm_client = self.sm_client.clone();
//        let outgoing_sender = self.outgoing_sender.clone();
//
//        let new_handshake_task = self.handshake_manager
//            .new_handshake(neighbor_public_key)
//            .into_future()
//            .map_err(|_| ChannelerError::HandshakeManagerError)
//            .and_then(move |request_nonce| {
//                sm_client.request_signature(Vec::from(request_nonce.as_ref()))
//                    .and_then(move |signature| {
//                        request_nonce.signature = signature;
//                        Ok(request_nonce)
//                    })
//            })
//            .and_then(|request_nonce| {
//                ChannelerMessage::RequestNonce(request_nonce)
//                    .encode()
//                    .into_future()
//                    .map_err(ChannelerError::Proto)
//                    .and_then(move |plain| {
//                        outgoing_sender
//                            .send((remote_addr, plain))
//                            .map_err(|_| ChannelerError::SendOutgoingError)
//                            .and_then(|_| Ok(()))
//                    })
//            });
//
//        self.handle.spawn(new_handshake_task.map_err(|e| {
//            info!("failed to process new handshake request: {:?}", e);
//        }));
//    }
//
//    fn process_request_nonce(&self, remote_addr: SocketAddr, request_nonce: RequestNonce) {
//        let outgoing_sender = self.outgoing_sender.clone();
//        let handshake_manager_sender = self.handshake_manager.clone();
//
//        let (response_sender, response_receiver) = oneshot::channel();
//
//        let message_to_handshake_manager = ToHandshakeManager::InitChannel {
//            init_channel,
//            response_sender,
//        };
//
//        let forward_task = handshake_manager_sender
//            .send(message_to_handshake_manager)
//            .map_err(|_| ChannelerError::HandshakeManagerError)
//            .and_then(move |_| {
//                response_receiver
//                    .map_err(|_| ChannelerError::HandshakeManagerError)
//                    .and_then(move |exchange_passive| {
//                        ChannelerMessage::ExchangePassive(exchange_passive)
//                            .encode()
//                            .into_future()
//                            .map_err(ChannelerError::Proto)
//                            .and_then(move |plain| {
//                                outgoing_sender
//                                    .send((remote_addr, plain))
//                                    .map_err(|_| ChannelerError::SendOutgoingError)
//                                    .and_then(|_| Ok(()))
//                            })
//                    })
//            });
//
//        self.handle.spawn(forward_task.map_err(|e| {
//            info!("failed to process init channel message: {:?}", e);
//        }));
//    }
//
//    fn process_channel_ready(&self, remote_addr: SocketAddr, channel_ready: ChannelReady) {
//        let channel_pool = Rc::clone(&self.channel_pool);
//        let handshake_manager_sender = self.handshake_manager.clone();
//
//        let (response_sender, response_receiver) = oneshot::channel();
//
//        let message_to_handshake_manager = ToHandshakeManager::ChannelReady {
//            channel_ready,
//            response_sender,
//        };
//
//        let forward_task = handshake_manager_sender
//            .send(message_to_handshake_manager)
//            .map_err(|_| ChannelerError::HandshakeManagerError)
//            .and_then(move |_| {
//                response_receiver
//                    .map_err(|_| ChannelerError::HandshakeManagerError)
//                    .and_then(move |new_channel_info| {
//                        channel_pool.borrow_mut().insert(remote_addr, new_channel_info);
//                        Ok(())
//                    })
//            });
//
//        self.handle.spawn(forward_task.map_err(|e| {
//            info!("failed to process channel ready message: {:?}", e);
//        }));
//    }
//
//    fn process_exchange_active(&self, remote_addr: SocketAddr, exchange_active: ExchangeActive) {
//        let channel_pool = Rc::clone(&self.channel_pool);
//        let outgoing_sender = self.outgoing_sender.clone();
//        let handshake_manager_sender = self.handshake_manager.clone();
//
//        let (response_sender, response_receiver) = oneshot::channel();
//
//        let message_to_handshake_manager = ToHandshakeManager::ExchangeActive {
//            exchange_active,
//            response_sender,
//        };
//
//        let forward_task = handshake_manager_sender
//            .send(message_to_handshake_manager)
//            .map_err(|_| ChannelerError::HandshakeManagerError)
//            .and_then(move |_| {
//                response_receiver
//                    .map_err(|_| ChannelerError::HandshakeManagerError)
//                    .and_then(move |(new_channel_info, channel_ready)| {
//                        ChannelerMessage::ChannelReady(channel_ready)
//                            .encode()
//                            .into_future()
//                            .map_err(ChannelerError::Proto)
//                            .and_then(move |plain| {
//                                outgoing_sender
//                                    .send((remote_addr, plain))
//                                    .map_err(|_| ChannelerError::SendOutgoingError)
//                                    .and_then(move |_| Ok(new_channel_info))
//                            })
//                    })
//                    .and_then(move |new_channel_info| {
//                        channel_pool.borrow_mut().insert(remote_addr, new_channel_info);
//                        Ok(())
//                    })
//            });
//
//        self.handle.spawn(forward_task.map_err(|e| {
//            info!("failed to process exchange active message: {:?}", e);
//        }));
//    }
//
//    fn process_exchange_passive(&self, remote_addr: SocketAddr, exchange_passive: ExchangePassive) {
//        let outgoing_sender = self.outgoing_sender.clone();
//        let handshake_manager_sender = self.handshake_manager.clone();
//
//        let (response_sender, response_receiver) = oneshot::channel();
//
//        let message_to_handshake_manager = ToHandshakeManager::ExchangePassive {
//            exchange_passive,
//            response_sender,
//        };
//
//        let forward_task = handshake_manager_sender
//            .send(message_to_handshake_manager)
//            .map_err(|_| ChannelerError::HandshakeManagerError)
//            .and_then(move |_| {
//                response_receiver
//                    .map_err(|_| ChannelerError::HandshakeManagerError)
//                    .and_then(move |exchange_active| {
//                        ChannelerMessage::ExchangeActive(exchange_active)
//                            .encode()
//                            .into_future()
//                            .map_err(ChannelerError::Proto)
//                            .and_then(move |plain| {
//                                outgoing_sender
//                                    .send((remote_addr, plain))
//                                    .map_err(|_| ChannelerError::SendOutgoingError)
//                                    .and_then(move |_| Ok(()))
//                            })
//                    })
//            });
//
//        self.handle.spawn(forward_task.map_err(|e| {
//            info!("failed to process exchange passive message: {:?}", e);
//        }));
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
//    fn start_send_outgoing(&mut self, item: (SocketAddr, Bytes)) -> Poll<(), ChannelerError> {
//        debug_assert!(self.outgoing_buffered.is_none());
//
//        let start_send_result = self.outgoing_sender.start_send(item)
//            .map_err(|_| ChannelerError::SendOutgoingError);
//
//        if let AsyncSink::NotReady(item) = start_send_result? {
//            self.outgoing_buffered = Some(item);
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
//            if let Some(outgoing_item) = self.outgoing_buffered.take() {
//                try_ready!( self.start_send_outgoing(outgoing_item));
//            }
//
//            if let Some(networker_item) = self.networker_buffered.take() {
//                try_ready!( self.start_send_networker(networker_item));
//            }
//
//// FIXME: Only terminated Channeler when encountered a fatal error!
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
//

// =============================== Helpers =============================

#[inline]
pub fn gen_random_bytes<SR>(rng: &SR, max_len: usize) -> Result<Bytes, ()>
    where SR: SecureRandom
{
    if (u16::max_value() as usize + 1) % max_len != 0 {
        Err(())
    } else {
        let mut len_bytes = [0x00; 2];
        rng.fill(&mut len_bytes[..]).map_err(|_| ())?;
        let len = BigEndian::read_u16(&len_bytes[..]) as usize % max_len + 1;

        let mut bytes = BytesMut::from(vec![0x00; len]);
        rng.fill(&mut bytes[..len]).map_err(|_| ())?;

        Ok(bytes.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn test_gen_random_bytes() {
        let fixed = FixedByteRandom { byte: 0x01 };
        let bytes = gen_random_bytes(&fixed, 32).unwrap();

        assert_eq!(bytes.len(), 2);
        assert!(bytes.iter().all(|x| *x == 0x01));
    }
}
