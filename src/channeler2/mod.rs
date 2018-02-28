use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::rc::Rc;

use bytes::{Bytes, BytesMut, Buf, BufMut, BigEndian};
use byteorder::ByteOrder;
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use ring::aead::{open_in_place, seal_in_place, OpeningKey, SealingKey};
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

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
// mod errors;
mod handshake;

use self::handshake::{HandshakeManager, ToHandshakeManager, HandshakeManagerError};

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const CHANNEL_ID_LEN: usize = 16;
const MAX_RAND_PADDING_LEN: usize = 32;

define_wrapped_bytes!(Nonce, NONCE_LEN);
define_wrapped_bytes!(ChannelId, CHANNEL_ID_LEN);

const RETRY_TICKS: usize = 100;

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

pub struct SendingEndState {
    remote_addr: SocketAddr,
    channel_id:  ChannelId,
    send_nonce:  Nonce,
    sealing_key: SealingKey,
}

pub struct ReceivingEndState {
    channel_id:    ChannelId,
//    recv_window:  NonceWindow,
opening_key: OpeningKey,
}

#[derive(Clone)]
pub struct NeighborInfo {
    pub public_key:  PublicKey,
    pub socket_addr: Option<SocketAddr>,
    pub retry_ticks: usize,
}

type NeighborsTable = HashMap<PublicKey, NeighborInfo>;

pub struct Channeler<T: Sink, R: Stream, SR: SecureRandom> {
    handle: Handle,

    /// Security module client.
    sm_client: SecurityModuleClient,

    /// Secure random number generator
    secure_rng: Rc<SR>,

    neighbors: Rc<RefCell<NeighborsTable>>,

    sending_ends:   HashMap<PublicKey, SendingEndState>,
    receiving_ends: HashMap<ChannelId, ReceivingEndState>,

    handshake_manager_sender: mpsc::Sender<ToHandshakeManager>,

    timer_receiver: mpsc::Receiver<FromTimer>,

    networker_sender:   mpsc::Sender<ChannelerToNetworker>,
    networker_buffered: Option<ChannelerToNetworker>,

    networker_receiver: mpsc::Receiver<NetworkerToChanneler>,

    outgoing_sender:   T,
    outgoing_buffered: Option<(SocketAddr, Bytes)>,

    incoming_receiver: R,
}

#[derive(Debug)]
pub enum ChannelerError {
    Io(io::Error),

    /// Error may occurred when encoding and decoding message
    Schema(SchemaError),

    /// Error may occurred when processing handshake messages
    HandshakeManager(HandshakeManagerError),

    RecvFromTimerError,
    TimerClosed,


    RecvFromNetworkerError,
    SendToNetworkerError,
    NetworkerClosed,

    EncryptionError,
    DecryptionError,

    RecvFromTimerFailed,
    SendToRemoteFailed,
    RecvFromOuterFailed,
    OuterStreamClosed,

    SecureRandomError,
    HandshakeManagerError,

    CloseReceiverCanceled,
    ClosingTaskCanceled,
    SendCloseNotificationFailed,
    NetworkerPollError,
}

impl<T, R, SR, TE, RE> Channeler<T, R, SR>
    where
        T: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
        R: Stream<Item=(SocketAddr, Bytes), Error=RE>,
        SR: SecureRandom,
        RE: Into<ChannelerError>,
        TE: Into<ChannelerError>,
{
    fn process_timer_next(&mut self) -> Poll<(), ChannelerError> {
        unimplemented!()
    }

    fn process_networker_next(&mut self) -> Poll<(), ChannelerError> {
        self.networker_receiver.poll()
            .map_err(|_| ChannelerError::RecvFromNetworkerError)
            .and_then(|item| match item {
                Async::NotReady => Ok(Async::NotReady),
                Async::Ready(None) => Err(ChannelerError::NetworkerClosed),
                Async::Ready(Some(networker_message)) => {
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
            })
    }

    fn process_incoming_next(&mut self) -> Poll<(), ChannelerError> {
        self.incoming_receiver.poll()
            .map_err(|_| ChannelerError::RecvFromOuterFailed)
            .and_then(|item| match item {
                Async::NotReady => Ok(Async::NotReady),
                Async::Ready(None) => Err(ChannelerError::RecvFromOuterFailed),
                Async::Ready(Some((remote_addr, plain))) => {
                    let channeler_message =
                        ChannelerMessage::decode(&plain).map_err(ChannelerError::Schema)?;

                    match channeler_message {
                        ChannelerMessage::Encrypted(encrypted) => {
                            unimplemented!()
                        }
                        ChannelerMessage::InitChannel(init_channel) => {
                            self.process_init_channel(remote_addr, init_channel);
                        }
                        ChannelerMessage::ChannelReady(channel_ready) => {
                            self.process_channel_ready(remote_addr, channel_ready);
                        }
                        ChannelerMessage::UnknownChannel(unknown_channel) => {
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
            })
    }

    // ===========================================================================================

//     fn retry_connect(&mut self) -> Poll<(), ChannelerError> {
//         let mut messages = Vec::new();
//
//         for (public_key, neighbor) in self.neighbors.borrow_mut().iter_mut() {
//             if let Some(socket_addr) = neighbor.socket_addr {
//                 if let Some(remaining) = neighbor.retry_ticks.checked_sub(1) {
//                     neighbor.retry_ticks = remaining;
//                 } else {
//                     neighbor.retry_ticks = RETRY_TICKS;
//
//                     let session_id = (public_key.clone(), Direction::Outgoing);
//
//                     if !self.pending.contains(&session_id) {
//                         let init = InitChannel {
//                             rand_nonce: RandValue::new(&*self.secure_rng),
//                             public_key: public_key.clone(),
//                         };
//                         let message = {
//                             let raw = ChannelerMessage::InitChannel(init)
//                                 .encode()
//                                 .map_err(ChannelerError::Schema)?;
//
//                             (socket_addr, raw)
//                         };
//
//                         messages.push((session_id, message));
//                     }
//                 }
//             }
//         }
//
//         for (session_id, message) in messages {
//             // Try to send the `InitChannel` message to remote, we
//             // do not buffer this message when the sender is busy.
//             try_ready!(self.start_send_outer(message, false));
//             debug_assert!(self.pending.insert(session_id));
//         }
//
//         Ok(Async::Ready(()))
//     }

    /// Add the given neighbor to channeler
    fn add_neighbor(&mut self, info: ChannelerNeighborInfo) {
        trace!("request to add neighbor: {:?}", info.public_key);

        unimplemented!()
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

        unimplemented!()
    }

    fn do_send_channel_message(
        &mut self,
        remote_public_key: PublicKey,
        content: Bytes
    ) -> Poll<(), ChannelerError> {
        let item = if let Some(sending_end) = self.sending_ends.get_mut(&remote_public_key) {
            let rand_padding = gen_random_bytes(&*self.secure_rng, MAX_RAND_PADDING_LEN)
                .map_err(|_| ChannelerError::SecureRandomError)?;

            let plain = Plain {
                rand_padding,
                content: PlainContent::User(content)
            };

            let encrypted_message = sealing_channel_message(sending_end, plain)
                .map_err(|_| ChannelerError::EncryptionError)?;

            let message = ChannelerMessage::Encrypted(encrypted_message)
                .encode().map_err(ChannelerError::Schema)?;

            (sending_end.remote_addr, message)
        } else {
            info!("no sending end for: {:?}", remote_public_key);
            return Ok(Async::Ready(()));
        };

        self.try_start_send_outgoing(item)
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
                                    .map_err(|_| ChannelerError::SendToRemoteFailed)
                                    .and_then(|_| Ok(()))
                            })
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process init channel message: {:?}", e);
        }));
    }

    fn process_channel_ready(&self, remote_addr: SocketAddr, channel_ready: ChannelReady) {
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
                    .and_then(|new_channel_info| {
                        // TODO: Constructs a new channel and add it into channeler
                        Ok(())
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process channel ready message: {:?}", e);
        }));
    }

    fn process_exchange_active(&self, remote_addr: SocketAddr, exchange_active: ExchangeActive) {
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
                                    .map_err(|_| ChannelerError::SendToRemoteFailed)
                                    .and_then(move |_| Ok(new_channel_info))
                            })
                    })
                    .and_then(|new_channel_info| {
                        // TODO: Constructs a new channel and add it into channeler
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
                                    .map_err(|_| ChannelerError::SendToRemoteFailed)
                                    .and_then(move |_| Ok(()))
                            })
                    })
            });

        self.handle.spawn(forward_task.map_err(|e| {
            info!("failed to process exchange passive message: {:?}", e);
        }));
    }

    fn try_start_send_networker(&mut self, item: ChannelerToNetworker) -> Poll<(), ChannelerError> {
        debug_assert!(self.networker_buffered.is_none());

        let start_send_result = self.networker_sender.start_send(item)
            .map_err(|_| ChannelerError::SendToNetworkerError);

        if let AsyncSink::NotReady(item) = start_send_result? {
            self.networker_buffered = Some(item);
            return Ok(Async::NotReady);
        }

        Ok(Async::Ready(()))
    }

    fn try_start_send_outgoing(&mut self, item: (SocketAddr, Bytes)) -> Poll<(), ChannelerError> {
        debug_assert!(self.outgoing_buffered.is_none());

        let start_send_result = self.outgoing_sender.start_send(item)
            .map_err(|_| ChannelerError::SendToRemoteFailed);

        if let AsyncSink::NotReady(item) = start_send_result? {
            self.outgoing_buffered = Some(item);
            return Ok(Async::NotReady);
        }

        Ok(Async::Ready(()))
    }
}

impl<T, R, SR, TE, RE> Future for Channeler<T, R, SR>
    where
        T: Sink<SinkItem=(SocketAddr, Bytes), SinkError=TE> + Clone + 'static,
        R: Stream<Item=(SocketAddr, Bytes), Error=RE>,
        SR: SecureRandom,
        TE: Into<ChannelerError>,
        RE: Into<ChannelerError>,
{
    type Item = ();
    type Error = ChannelerError;

    fn poll(&mut self) -> Poll<(), ChannelerError> {
        loop {
            if let Some(outgoing_item) = self.outgoing_buffered.take() {
                try_ready!(self.try_start_send_outgoing(outgoing_item));
            }

            if let Some(networker_item) = self.networker_buffered.take() {
                try_ready!(self.try_start_send_networker(networker_item));
            }

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

// ===== helper functions =====
// TODO A proper place for these

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

#[inline]
pub fn sealing_channel_message(
    sending_end: &mut SendingEndState,
    plain_message: Plain,
) -> Result<Bytes, ()> {
    let serialized_plain = plain_message.encode().map_err(|_| ())?;

    increase_nonce(&mut sending_end.send_nonce);

    let mut buffer = BytesMut::with_capacity(NONCE_LEN + serialized_plain.len() + TAG_LEN);

    buffer.extend_from_slice(&sending_end.send_nonce);
    buffer.extend_from_slice(&serialized_plain);
    buffer.extend_from_slice(&[0x00; TAG_LEN][..]);

    let key   = &sending_end.sealing_key;
    let nonce = &sending_end.send_nonce;
    let ad    = &sending_end.channel_id;

    seal_in_place(key, nonce, ad, &mut buffer[NONCE_LEN..], TAG_LEN)
        .map_err(|_| ()).and_then(move |sz| Ok(buffer.split_to(NONCE_LEN + sz).freeze()))
}

#[inline]
pub fn opening_channel_message(
    receiving_end: &mut ReceivingEndState,
    mut encrypted_message: Bytes
) -> Result<Plain, ()> {
    let nonce = encrypted_message.split_to(NONCE_LEN);

    // FIXME: test nonce
    // if !receiving_end.recv_window.try_accept(&nonce) {
    //     Err(())
    // } else {
        let key = &receiving_end.opening_key;
        let ad  = &receiving_end.channel_id;
        open_in_place(key, &nonce, ad, 0, &mut BytesMut::from(encrypted_message)).map_err(|_| ())
            .and_then(|serialized_plain| {
                Plain::decode(serialized_plain).map_err(|_| ())
            })
    // }
}

/// Increase the bytes represented number by 1.
#[inline]
pub fn increase_nonce(nonce: &mut [u8]) {
    let mut c: u16 = 1;
    for i in nonce {
        c += u16::from(*i);
        *i = c as u8;
        c >>= 8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;
    use ring::aead::CHACHA20_POLY1305;
    use ring::test::rand::FixedByteRandom;

    #[test]
    fn test_gen_random_bytes() {
        let fixed = FixedByteRandom { byte: 0x01 };
        let bytes = gen_random_bytes(&fixed, 32).unwrap();

        assert_eq!(bytes.len(), 2);
        assert!(bytes.iter().all(|x| *x == 0x01));
    }

    #[test]
    fn test_sealing_opening_good() {
        let fixed = FixedByteRandom { byte: 0x01 };

        let mut sending_end = SendingEndState {
            remote_addr: "127.0.0.1:10001".parse().unwrap(),
            channel_id: ChannelId::try_from(&[0x00u8; CHANNEL_ID_LEN][..]).unwrap(),
            send_nonce: Nonce::try_from(&[0x01u8; NONCE_LEN][..]).unwrap(),
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &[0x03u8; 32][..]).unwrap(),
        };

        let mut receiving_end = ReceivingEndState {
            channel_id: ChannelId::try_from(&[0x00u8; CHANNEL_ID_LEN][..]).unwrap(),
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x03u8; 32][..]).unwrap(),
        };

        let plain = Plain {
            rand_padding: gen_random_bytes(&fixed, MAX_RAND_PADDING_LEN).unwrap(),
            content: PlainContent::User(Bytes::from("hello!")),
        };

        let encrypted = sealing_channel_message(&mut sending_end, plain).unwrap();
        let plain = opening_channel_message(&mut receiving_end, encrypted).unwrap();

        assert_eq!(plain.content, PlainContent::User(Bytes::from("hello!")));
    }

    #[test]
    #[should_panic]
    fn test_sealing_opening_bad_channel_id() {
        let fixed = FixedByteRandom { byte: 0x01 };

        let mut sending_end = SendingEndState {
            remote_addr: "127.0.0.1:10001".parse().unwrap(),
            channel_id: ChannelId::try_from(&[0x00u8; CHANNEL_ID_LEN][..]).unwrap(),
            send_nonce: Nonce::try_from(&[0x01u8; NONCE_LEN][..]).unwrap(),
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &[0x03u8; 32][..]).unwrap(),
        };

        let mut receiving_end = ReceivingEndState {
            channel_id: ChannelId::try_from(&[0xffu8; CHANNEL_ID_LEN][..]).unwrap(),
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x03u8; 32][..]).unwrap(),
        };

        let plain = Plain {
            rand_padding: gen_random_bytes(&fixed, MAX_RAND_PADDING_LEN).unwrap(),
            content: PlainContent::User(Bytes::from("hello!")),
        };

        let encrypted = sealing_channel_message(&mut sending_end, plain).unwrap();
        let plain = opening_channel_message(&mut receiving_end, encrypted).unwrap();

        assert_eq!(plain.content, PlainContent::User(Bytes::from("hello!")));
    }
}
