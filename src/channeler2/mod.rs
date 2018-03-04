use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::rc::Rc;

use bytes::{Bytes, BytesMut, Buf, BufMut, BigEndian};
use byteorder::{ByteOrder, LittleEndian};
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

// FIXME
use channeler::types::ChannelerNeighborInfo;
//use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
//use crypto::hash::{HashResult, sha_512_256};
use crypto::identity::PublicKey;
//use crypto::rand_values::RandValue;
use security_module::client::SecurityModuleClient;
use timer::messages::FromTimer;

use proto::{Schema, SchemaError};
use networker::messages::NetworkerToChanneler;

use proto::channeler_udp::{
    ChannelReady, ChannelerMessage, ExchangeActive, ExchangePassive,
    InitChannel, PlainContent, Plain,
};

#[macro_use]
mod macros;
mod channel;
mod handshake;

pub use self::channel::{ChannelId, CHANNEL_ID_LEN};
use self::channel::{Channel, NewChannelInfo, ChannelConfig};
use self::handshake::{HandshakeManager, ToHandshakeManager, HandshakeManagerError};

// TODO: Introduce `ChannelerConfig`
const TAG_LEN: usize = 16;
const RETRY_TICKS: usize = 100;
const MAX_RAND_PADDING_LEN: usize = 32;

/// The channel event expected to be sent to `Networker`.
pub enum ChannelEvent {
    /// A message received from remote.
    Message(Bytes),
}

/// The internal message expected to be sent to `Networker`.
pub struct ChannelerToNetworker {
    /// The public key of the event sender.
    pub remote_public_key: PublicKey,

    /// The event happened.
    pub event: ChannelEvent,
}

#[derive(Clone)]
pub struct NeighborInfo {
    pub public_key:  PublicKey,
    pub socket_addr: Option<SocketAddr>,
    pub retry_ticks: usize,
}

type ChannelsTable  = HashMap<PublicKey, Channel>;
type NeighborsTable = HashMap<PublicKey, NeighborInfo>;

pub struct Channeler<I: Stream, O: Sink, SR: SecureRandom> {
    handle: Handle,

    /// Security module client.
    sm_client: SecurityModuleClient,

    /// Secure random number generator
    secure_rng: Rc<SR>,

    channels:  Rc<RefCell<ChannelsTable>>,
    neighbors: Rc<RefCell<NeighborsTable>>,

    handshake_manager_sender: mpsc::Sender<ToHandshakeManager>,

    timer_receiver: mpsc::Receiver<FromTimer>,

    networker_sender:   mpsc::Sender<ChannelerToNetworker>,
    networker_buffered: Option<ChannelerToNetworker>,

    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,

    outgoing_sender: O,
    outgoing_buffered: Option<(SocketAddr, Bytes)>,

    incoming_receiver: I,
}

// NOTE: When an error returned by Stream::poll, it not a fatal error for futures 0.1
// check https://github.com/rust-lang-nursery/futures-rs/issues/206 for more details.
#[derive(Debug)]
pub enum ChannelerError {
    Io(io::Error),

    Schema(SchemaError),

    PollTimerError,
    TimerTerminated,      // fatal error

    PollNetworkerError,
    SendToNetworkerError, // fatal error
    NetworkerTerminated,  // fatal error

    PollIncomingError,
    SendOutgoingError,    // fatal error
    IncomingTerminated,   // fatal error

    SecureRandomError,
    PrepareSendError,

    SendToHandshakeManagerError, // fatal error

    HandshakeManagerError,
}

impl<I, O, SR, TE, RE> Channeler<I, O, SR>
    where
        I: Stream<Item=(SocketAddr, Bytes), Error=RE>,
        O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
        SR: SecureRandom,
        RE: Into<ChannelerError>,
        TE: Into<ChannelerError>,
{
    fn process_timer_next(&mut self) -> Poll<(), ChannelerError> {
        let poll_result = self.timer_receiver.poll()
            .map_err(|_| ChannelerError::PollTimerError);

        match try_ready!(poll_result) {
            None => Err(ChannelerError::TimerTerminated),
            Some(FromTimer::TimeTick) => {
                for neighbor in self.neighbors.borrow_mut().values_mut() {
                    if let Some(remote_addr) = neighbor.socket_addr {
                        if neighbor.retry_ticks <= 1 {
                            neighbor.retry_ticks = RETRY_TICKS;
                            self.process_new_handshake(remote_addr, neighbor.public_key.clone());
                        } else {
                            neighbor.retry_ticks -= 1;
                        }
                    }
                }

                Ok(Async::Ready(()))
            }
        }
    }

    fn process_networker_next(&mut self) -> Poll<(), ChannelerError> {
        let poll_result = self.networker_receiver.poll()
            .map_err(|_| ChannelerError::PollNetworkerError);

        match try_ready!(poll_result) {
            None => Err(ChannelerError::NetworkerTerminated),
            Some(networker_message) => {
                match networker_message {
                    NetworkerToChanneler::AddNeighbor { info } => {
                        self.add_neighbor(info);
                    },
                    NetworkerToChanneler::RemoveNeighbor { public_key } => {
                        self.remove_neighbor(public_key);
                    },
                    NetworkerToChanneler::SendChannelMessage {
                        neighbor_public_key,
                        content,
                    } => {
                        return self.do_send_channel_message(neighbor_public_key, content);
                    }
                }

                Ok(Async::Ready(()))
            }
        }
    }

    fn process_incoming_next(&mut self) -> Poll<(), ChannelerError> {
        let poll_result = self.incoming_receiver.poll()
            .map_err(|_| ChannelerError::PollIncomingError);

        match try_ready!(poll_result) {
            None => Err(ChannelerError::IncomingTerminated),
            Some((remote_addr, plain)) => {
                let channeler_message = ChannelerMessage::decode(&plain)
                    .map_err(ChannelerError::Schema)?;

                match channeler_message {
                    ChannelerMessage::Encrypted(encrypted) => {
                        // FIXME: This actually block the channeler!
                        // FIXME: We may borrow the channels too long here!
                        let opt_message = {
                            let mut res = None;

                            for (remote_public_key, channel) in self.channels.borrow_mut().iter_mut() {
                                match channel.try_recv(encrypted.clone()) {
                                    Ok(None) => break,
                                    Ok(Some(content)) => {
                                        let message_to_networker = ChannelerToNetworker {
                                            remote_public_key: remote_public_key.clone(),
                                            event: ChannelEvent::Message(content),
                                        };

                                        res = Some(message_to_networker);
                                        break;
                                    }
                                    Err(_) => continue,
                                }
                            }

                            res
                        };

                        if let Some(message) = opt_message {
                            return self.start_send_networker(message);
                        }
                    }
                    ChannelerMessage::InitChannel(init_channel) => {
                        self.process_init_channel(remote_addr, init_channel);
                    }
                    ChannelerMessage::ChannelReady(channel_ready) => {
                        self.process_channel_ready(remote_addr, channel_ready);
                    }
                    ChannelerMessage::UnknownChannel(_unknown_channel) => {
                        unimplemented!()
                    }
                    ChannelerMessage::ExchangeActive(exchange_active) => {
                        self.process_exchange_active(remote_addr, exchange_active);
                    }
                    ChannelerMessage::ExchangePassive(exchange_passive) => {
                        self.process_exchange_passive(remote_addr, exchange_passive);
                    }
                }

                Ok(Async::Ready(()))
            }
        }
    }

    // ===========================================================================================

    fn add_neighbor(&mut self, info: ChannelerNeighborInfo) {
        trace!("request to add neighbor: {:?}", info.public_key);

        if !self.neighbors.borrow().contains_key(&info.public_key) {
            let new_neighbor = NeighborInfo {
                public_key: info.public_key.clone(),
                socket_addr: info.socket_addr,
                retry_ticks: 0,
            };
            self.neighbors.borrow_mut().insert(info.public_key, new_neighbor);
        } else {
            info!("neighbor: {:?} exist, do nothing", info.public_key);
        }
    }


    /// Remove specified neighbor from channeler
    ///
    /// For removing a channeler neighbor, we **SHOULD** have the following done:
    ///
    /// - Remove the neighbor's information from neighbors table.
    /// - Remove the sending end from the sending ends table.
    /// - Remove all the relevant receiving ends from the receiving ends table.
    fn remove_neighbor(&mut self, public_key: PublicKey) {
        trace!("request to remove neighbor: {:?}", public_key);

        self.channels.borrow_mut().remove(&public_key);
        self.neighbors.borrow_mut().remove(&public_key);
    }

    fn do_send_channel_message(
        &mut self,
        remote_public_key: PublicKey,
        content: Bytes
    ) -> Poll<(), ChannelerError> {
        let item = if let Some(channel) = self.channels.borrow_mut().get_mut(&remote_public_key) {
            let rand_padding = gen_random_bytes(&*self.secure_rng, MAX_RAND_PADDING_LEN)
                .map_err(|_| ChannelerError::SecureRandomError)?;

            let plain = Plain {
                rand_padding,
                content: PlainContent::User(content)
            };

            let (addr, channeler_message) = channel.pre_send(plain)
                .map_err(|_| ChannelerError::PrepareSendError)?;

            let serialized_message = channeler_message.encode().map_err(ChannelerError::Schema)?;


            (addr, serialized_message)
        } else {
            info!("no sending end for: {:?}", remote_public_key);
            return Ok(Async::Ready(()));
        };

        self.start_send_outgoing(item)
    }

    fn process_new_handshake(&self, remote_addr: SocketAddr, neighbor_public_key: PublicKey) {
        let outgoing_sender = self.outgoing_sender.clone();
        let handshake_manager_sender = self.handshake_manager_sender.clone();

        let (response_sender, response_receiver) = oneshot::channel();

        let message_to_handshake_manager = ToHandshakeManager::NewHandshake {
            neighbor_public_key,
            response_sender,
        };

        let forward_task = handshake_manager_sender
            .send(message_to_handshake_manager)
            .map_err(|_| ChannelerError::HandshakeManagerError)
            .and_then(move |_| {
                response_receiver
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |init_channel| {
                        ChannelerMessage::InitChannel(init_channel)
                            .encode()
                            .into_future()
                            .map_err(ChannelerError::Schema)
                            .and_then(move |plain| {
                                outgoing_sender
                                    .send((remote_addr, plain))
                                    .map_err(|_| ChannelerError::SendOutgoingError)
                                    .and_then(|_| Ok(()))
                            })
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process new handshake request: {:?}", e);
        }))
    }

    fn process_init_channel(&self, remote_addr: SocketAddr, init_channel: InitChannel) {
        let outgoing_sender = self.outgoing_sender.clone();
        let handshake_manager_sender = self.handshake_manager_sender.clone();

        let (response_sender, response_receiver) = oneshot::channel();

        let message_to_handshake_manager = ToHandshakeManager::InitChannel {
            init_channel,
            response_sender,
        };

        let forward_task = handshake_manager_sender
            .send(message_to_handshake_manager)
            .map_err(|_| ChannelerError::HandshakeManagerError)
            .and_then(move |_| {
                response_receiver
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |exchange_passive| {
                        ChannelerMessage::ExchangePassive(exchange_passive)
                            .encode()
                            .into_future()
                            .map_err(ChannelerError::Schema)
                            .and_then(move |plain| {
                                outgoing_sender
                                    .send((remote_addr, plain))
                                    .map_err(|_| ChannelerError::SendOutgoingError)
                                    .and_then(|_| Ok(()))
                            })
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process init channel message: {:?}", e);
        }));
    }

    fn process_channel_ready(&self, remote_addr: SocketAddr, channel_ready: ChannelReady) {
        let channels = Rc::clone(&self.channels);
        let handshake_manager_sender = self.handshake_manager_sender.clone();

        let (response_sender, response_receiver) = oneshot::channel();

        let message_to_handshake_manager = ToHandshakeManager::ChannelReady {
            channel_ready,
            response_sender,
        };

        let forward_task = handshake_manager_sender
            .send(message_to_handshake_manager)
            .map_err(|_| ChannelerError::HandshakeManagerError)
            .and_then(move |_| {
                response_receiver
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |new_channel_info| {
                        add_channel(channels, remote_addr, new_channel_info);
                        Ok(())
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process channel ready message: {:?}", e);
        }));
    }

    fn process_exchange_active(&self, remote_addr: SocketAddr, exchange_active: ExchangeActive) {
        let channels = Rc::clone(&self.channels);
        let outgoing_sender = self.outgoing_sender.clone();
        let handshake_manager_sender = self.handshake_manager_sender.clone();

        let (response_sender, response_receiver) = oneshot::channel();

        let message_to_handshake_manager = ToHandshakeManager::ExchangeActive {
            exchange_active,
            response_sender,
        };

        let forward_task = handshake_manager_sender
            .send(message_to_handshake_manager)
            .map_err(|_| ChannelerError::HandshakeManagerError)
            .and_then(move |_| {
                response_receiver
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |(new_channel_info, channel_ready)| {
                        ChannelerMessage::ChannelReady(channel_ready)
                            .encode()
                            .into_future()
                            .map_err(ChannelerError::Schema)
                            .and_then(move |plain| {
                                outgoing_sender
                                    .send((remote_addr, plain))
                                    .map_err(|_| ChannelerError::SendOutgoingError)
                                    .and_then(move |_| Ok(new_channel_info))
                            })
                    })
                    .and_then(move |new_channel_info| {
                        add_channel(channels, remote_addr, new_channel_info);
                        Ok(())
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process exchange active message: {:?}", e);
        }));
    }

    fn process_exchange_passive(&self, remote_addr: SocketAddr, exchange_passive: ExchangePassive) {
        let outgoing_sender = self.outgoing_sender.clone();
        let handshake_manager_sender = self.handshake_manager_sender.clone();

        let (response_sender, response_receiver) = oneshot::channel();

        let message_to_handshake_manager = ToHandshakeManager::ExchangePassive {
            exchange_passive,
            response_sender,
        };

        let forward_task = handshake_manager_sender
            .send(message_to_handshake_manager)
            .map_err(|_| ChannelerError::HandshakeManagerError)
            .and_then(move |_| {
                response_receiver
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |exchange_active| {
                        ChannelerMessage::ExchangeActive(exchange_active)
                            .encode()
                            .into_future()
                            .map_err(ChannelerError::Schema)
                            .and_then(move |plain| {
                                outgoing_sender
                                    .send((remote_addr, plain))
                                    .map_err(|_| ChannelerError::SendOutgoingError)
                                    .and_then(move |_| Ok(()))
                            })
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process exchange passive message: {:?}", e);
        }));
    }

    fn start_send_networker(&mut self, item: ChannelerToNetworker) -> Poll<(), ChannelerError> {
        debug_assert!(self.networker_buffered.is_none());

        let start_send_result = self.networker_sender.start_send(item)
            .map_err(|_| ChannelerError::SendToNetworkerError);

        if let AsyncSink::NotReady(item) = start_send_result? {
            self.networker_buffered = Some(item);
            return Ok(Async::NotReady);
        }

        Ok(Async::Ready(()))
    }

    fn start_send_outgoing(&mut self, item: (SocketAddr, Bytes)) -> Poll<(), ChannelerError> {
        debug_assert!(self.outgoing_buffered.is_none());

        let start_send_result = self.outgoing_sender.start_send(item)
            .map_err(|_| ChannelerError::SendOutgoingError);

        if let AsyncSink::NotReady(item) = start_send_result? {
            self.outgoing_buffered = Some(item);
            return Ok(Async::NotReady);
        }

        Ok(Async::Ready(()))
    }
}

impl<I, O, SR, TE, RE> Future for Channeler<I, O, SR>
    where
        I: Stream<Item=(SocketAddr, Bytes), Error=RE>,
        O: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
        SR: SecureRandom,
        TE: Into<ChannelerError>,
        RE: Into<ChannelerError>,
{
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<(), ChannelerError> {
        loop {
            if let Some(outgoing_item) = self.outgoing_buffered.take() {
                try_ready!(self.start_send_outgoing(outgoing_item));
            }

            if let Some(networker_item) = self.networker_buffered.take() {
                try_ready!(self.start_send_networker(networker_item));
            }

            // FIXME: Only terminated Channeler when encountered a fatal error!
            if let Async::NotReady = self.process_timer_next()? {
                if let Async::NotReady = self.process_networker_next()? {
                    if let Async::NotReady = self.process_incoming_next()? {
                        return Ok(Async::NotReady);
                    }
                }
            }
        }
    }
}

fn add_channel(channels: Rc<RefCell<ChannelsTable>>, remote_addr: SocketAddr, info: NewChannelInfo) {
    // FIXME: Find a proper way to get the config
    let config = ChannelConfig {
        max_recv_end: 3,
        recv_wnd_size: 256,
        keepalive_ticks: 100,
    };

    let remote_public_key = info.remote_public_key.clone();

    {
        let mut channels = channels.borrow_mut();

        if let Some(channel) = channels.get_mut(&remote_public_key) {
            channel.replace(remote_addr, info);
        } else {
            let new_channel = Channel::new(remote_addr, info, config);
            channels.insert(remote_public_key, new_channel);
        }
    }
}

/// Generate a random byte sequence.
#[inline]
pub fn gen_random_bytes<R: SecureRandom>(rng: &R, max_len: usize) -> Result<Bytes, ()> {
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
