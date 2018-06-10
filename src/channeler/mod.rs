use std::rc::Rc;
use std::cell::RefCell;

use std::io;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut, BigEndian};
use byteorder::ByteOrder;
use futures::prelude::*;
use futures::sync::mpsc;
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use crypto::identity::PublicKey;
use security_module::client::SecurityModuleClient;
use timer::messages::FromTimer;

use proto::{Proto, ProtoError};
use networker::messages::NetworkerToChanneler;

use proto::channeler::{
    ChannelerMessage,
    RequestNonce,
    ResponseNonce,
    ExchangeActive,
    ExchangePassive,
    ChannelReady,
    PlainContent,
    Plain
};

use self::channel::{ChannelPool, ChannelError};
use self::handshake::HandshakeService;
use self::types::{NeighborTable, ChannelerNeighbor, ChannelerNeighborInfo};
use self::messages::{ChannelEvent, ChannelerToNetworker};

pub mod types;
pub mod config;
pub mod channel;
pub mod messages;
pub mod handshake;

const RETRY_TICKS: usize = 100;
const MAX_RAND_PADDING_LEN: usize = 32;

pub struct Channeler<I: Stream, O: Sink, SR: SecureRandom> {
    handle: Handle,

    /// Security module client.
    sm_client: SecurityModuleClient,

    /// Secure random number generator
    secure_rng: Rc<SR>,

    channel_pool: Rc<RefCell<ChannelPool>>,
    neighbors: Rc<RefCell<NeighborTable>>,

    handshake_manager: HandshakeService<SR>,

    timer_receiver: mpsc::Receiver<FromTimer>,

    networker_sender: mpsc::Sender<ChannelerToNetworker>,
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
    Proto(ProtoError),
    Channel(ChannelError),

    PollTimerError,
    TimerTerminated,      // fatal error

    PollNetworkerError,
    SendToNetworkerError,
    NetworkerTerminated,  // fatal error

    PollIncomingError,
    SendOutgoingError,
    IncomingTerminated,   // fatal error

    SecureRandomError,
    PrepareSendError,

    SendToHandshakeManagerError, // fatal error

    HandshakeManagerError,
}

impl From<ChannelError> for ChannelerError {
    fn from(e: ChannelError) -> ChannelerError {
        ChannelerError::Channel(e)
    }
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
        let polled_timer_msg = self.timer_receiver.poll()
            .map_err(|_| ChannelerError::PollTimerError);

        match try_ready!(polled_timer_msg) {
            None => Err(ChannelerError::TimerTerminated),
            Some(FromTimer::TimeTick) => {
                let mut reconnect = Vec::new();
                for neighbor in self.neighbors.borrow_mut().values_mut() {
                    if let Some(remote_addr) = neighbor.info.socket_addr {
                        if !self.channel_pool.borrow().is_connected_to(
                            &neighbor.info.public_key
                        ) {
                            if neighbor.retry_ticks <= 1 {
                                neighbor.retry_ticks = RETRY_TICKS;
                                reconnect.push((
                                    remote_addr,
                                    neighbor.info.public_key.clone()
                                ));
                            } else {
                                neighbor.retry_ticks -= 1;
                            }
                        }
                    }
                }
                for (addr, pk) in reconnect {
                    self.process_new_handshake(addr, pk);
                }

                self.handshake_manager.time_tick();

                let _should_send_keepalive =
                    self.channel_pool.borrow_mut().time_tick();
                // TODO: Send keepalive message

                Ok(Async::Ready(()))
            }
        }
    }

    fn process_networker_next(&mut self) -> Poll<(), ChannelerError> {
        let polled_networker_msg = self.networker_receiver.poll()
            .map_err(|_| ChannelerError::PollNetworkerError);

        match try_ready!(polled_networker_msg) {
            None => Err(ChannelerError::NetworkerTerminated),
            Some(networker_message) => {
                match networker_message {
                    NetworkerToChanneler::AddNeighbor { info } => {
                        self.add_neighbor(info);
                    },
                    NetworkerToChanneler::RemoveNeighbor {
                        neighbor_public_key
                    } => {
                        self.remove_neighbor(neighbor_public_key);
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
        let polled_incoming_msg = self.incoming_receiver.poll()
            .map_err(|_| ChannelerError::PollIncomingError);

        match try_ready!(polled_incoming_msg) {
            None => Err(ChannelerError::IncomingTerminated),
            Some((remote_addr, plain)) => {
                let channeler_message = ChannelerMessage::decode(&plain)
                    .map_err(ChannelerError::Proto)?;

                match channeler_message {
                    ChannelerMessage::Encrypted(encrypted) => {
                        let opt_message =
                            self.channel_pool.borrow_mut().decrypt_message(encrypted)?;

                        if let (pk, Some(message)) = opt_message {
                            let message_to_networker = ChannelerToNetworker {
                                remote_public_key: pk,
                                event: ChannelEvent::Message(message),
                            };
                            return self.start_send_networker(message_to_networker);
                        }
                    }
                    ChannelerMessage::RequestNonce(request_nonce) => {
                        self.process_request_nonce(remote_addr, request_nonce);
                    }
                    ChannelerMessage::ResponseNonce(respond_nonce) => {
                        self.process_respond_nonce(remote_addr, respond_nonce);
                    },
                    ChannelerMessage::ExchangeActive(exchange_active) => {
                        self.process_exchange_active(remote_addr, exchange_active);
                    }
                    ChannelerMessage::ExchangePassive(exchange_passive) => {
                        self.process_exchange_passive(remote_addr, exchange_passive);
                    }
                    ChannelerMessage::ChannelReady(channel_ready) => {
                        self.process_channel_ready(remote_addr, channel_ready);
                    }
                    ChannelerMessage::UnknownChannel(_unknown_channel) => {
                        unimplemented!()
                    }
                }

                Ok(Async::Ready(()))
            }
        }
    }

    // ===========================================================================================

    fn add_neighbor(&mut self, info: ChannelerNeighborInfo) {
        let public_key = info.public_key.clone();
        trace!("request to add neighbor: {:?}", public_key);

        if !self.neighbors.borrow().contains_key(&public_key) {
            let new_neighbor = ChannelerNeighbor {
                info,
                retry_ticks: 0,
            };
            self.neighbors.borrow_mut().insert(public_key, new_neighbor);
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

        self.channel_pool.borrow_mut().remove_channel(&public_key);
        self.neighbors.borrow_mut().remove(&public_key);
    }

    fn do_send_channel_message(&mut self, public_key: PublicKey, content: Bytes)
        -> Poll<(), ChannelerError>
    {
        let rand_padding = gen_random_bytes(MAX_RAND_PADDING_LEN, &*self.secure_rng)
            .map_err(|_| ChannelerError::SecureRandomError)?;

        let plain = Plain {
            rand_padding,
            content: PlainContent::Application(content)
        };

        let item = self.channel_pool.borrow_mut().encrypt_message(&public_key, plain)?;

        self.start_send_outgoing(item)
    }

    fn process_new_handshake(
        &mut self,
        remote_addr: SocketAddr,
        neighbor_public_key: PublicKey
    ) {
        let outgoing_sender = self.outgoing_sender.clone();

        let new_handshake_task = self.handshake_manager
            .initiate_handshake(neighbor_public_key)
            .into_future()
            .map_err(|e| {
                error!("handshake manager error: {:?}", e);
                ChannelerError::HandshakeManagerError
            })
            .and_then(move |request_nonce| {
                ChannelerMessage::RequestNonce(request_nonce)
                    .encode()
                    .into_future()
                    .map_err(ChannelerError::Proto)
                    .and_then(move |plain| {
                        outgoing_sender
                            .send((remote_addr, plain))
                            .map_err(|_| ChannelerError::SendOutgoingError)
                            .and_then(|_| Ok(()))
                    })
            });

        self.handle.spawn(new_handshake_task.map_err(|e| {
            info!("failed to process new handshake request: {:?}", e);
        }));
    }

    fn process_request_nonce(
        &mut self,
        remote_addr: SocketAddr,
        request_nonce: RequestNonce
    ) {
        let sm_client = self.sm_client.clone();
        let outgoing_sender = self.outgoing_sender.clone();

        let process_request_nonce_task = self.handshake_manager
            .process_request_nonce(request_nonce)
            .into_future()
            .map_err(|e| {
                error!("handshake manager error: {:?}", e);
                ChannelerError::HandshakeManagerError
            })
            .and_then(move |mut respond_nonce| {
                sm_client.request_signature(respond_nonce.as_bytes().to_vec())
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |signature| {
                        respond_nonce.signature = signature;

                        Ok(respond_nonce)
                    })
            })
            .and_then(move |respond_nonce| {
                ChannelerMessage::ResponseNonce(respond_nonce)
                    .encode()
                    .into_future()
                    .map_err(ChannelerError::Proto)
                    .and_then(move |plain| {
                        outgoing_sender
                            .send((remote_addr, plain))
                            .map_err(|_| ChannelerError::SendOutgoingError)
                            .and_then(|_| Ok(()))
                    })
            });

        self.handle.spawn(process_request_nonce_task.map_err(|e| {
            info!("failed to process request nonce message: {:?}", e);
        }));
    }

    fn process_respond_nonce(
        &mut self,
        remote_addr: SocketAddr,
        respond_nonce: ResponseNonce
    ) {
        let sm_client = self.sm_client.clone();
        let outgoing_sender = self.outgoing_sender.clone();

        let process_respond_nonce_task = self.handshake_manager
            .process_response_nonce(respond_nonce)
            .into_future()
            .map_err(|e| {
                error!("handshake manager error: {:?}", e);
                ChannelerError::HandshakeManagerError
            })
            .and_then(move |mut exchange_active| {
                sm_client.request_signature(exchange_active.as_bytes().to_vec())
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |signature| {
                        exchange_active.signature = signature;

                        Ok(exchange_active)
                    })
            })
            .and_then(move |exchange_active| {
                ChannelerMessage::ExchangeActive(exchange_active)
                    .encode()
                    .into_future()
                    .map_err(ChannelerError::Proto)
                    .and_then(move |plain| {
                        outgoing_sender
                            .send((remote_addr, plain))
                            .map_err(|_| ChannelerError::SendOutgoingError)
                            .and_then(|_| Ok(()))
                    })
            });

        self.handle.spawn(process_respond_nonce_task.map_err(|e| {
            info!("failed to process respond nonce message: {:?}", e);
        }));
    }

    fn process_channel_ready(
        &mut self,
        remote_addr: SocketAddr,
        channel_ready: ChannelReady
    ) {
        let channel_pool = Rc::clone(&self.channel_pool);

        match self.handshake_manager.process_channel_ready(channel_ready) {
            Ok(handshake_res) => {
                channel_pool.borrow_mut().add_channel(remote_addr, handshake_res)
            },
            Err(e) => {
                error!("handshake manager error: {:?}", e);
            }
        }
    }

    fn process_exchange_active(
        &mut self,
        remote_addr: SocketAddr,
        exchange_active: ExchangeActive
    ) {
        let sm_client = self.sm_client.clone();
        let outgoing_sender = self.outgoing_sender.clone();

        let process_exchange_active_task = self.handshake_manager
            .process_exchange_active(exchange_active)
            .into_future()
            .map_err(|e| {
                error!("handshake manager error: {:?}", e);
                ChannelerError::HandshakeManagerError
            })
            .and_then(move |mut exchange_passive| {
                sm_client.request_signature(exchange_passive.as_bytes().to_vec())
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |signature| {
                        exchange_passive.signature = signature;

                        Ok(exchange_passive)
                    })
            })
            .and_then(move |exchange_passive| {
                ChannelerMessage::ExchangePassive(exchange_passive)
                    .encode()
                    .into_future()
                    .map_err(ChannelerError::Proto)
                    .and_then(move |plain| {
                        outgoing_sender
                            .send((remote_addr, plain))
                            .map_err(|_| ChannelerError::SendOutgoingError)
                            .and_then(move |_| Ok(()))
                    })
            });

        self.handle.spawn(process_exchange_active_task.map_err(|e| {
            info!("failed to process exchange active message: {:?}", e);
        }));
    }

    fn process_exchange_passive(
        &mut self,
        remote_addr: SocketAddr,
        exchange_passive: ExchangePassive
    ) {
        let sm_client = self.sm_client.clone();
        let channel_pool = Rc::clone(&self.channel_pool);
        let outgoing_sender = self.outgoing_sender.clone();

        let process_exchange_passive_task = self.handshake_manager
            .process_exchange_passive(exchange_passive)
            .into_future()
            .map_err(|e| {
                error!("handshake manager error: {:?}", e);
                ChannelerError::HandshakeManagerError
            })
            .and_then(move |(handshake_res, mut channel_ready)| {
                sm_client.request_signature(channel_ready.as_bytes().to_vec())
                    .map_err(|_| ChannelerError::HandshakeManagerError)
                    .and_then(move |signature| {
                        channel_ready.signature = signature;

                        Ok((handshake_res, channel_ready))
                    })
            })
            .and_then(move |(handshake_res, channel_ready)| {
                ChannelerMessage::ChannelReady(channel_ready)
                    .encode()
                    .into_future()
                    .map_err(ChannelerError::Proto)
                    .and_then(move |plain| {
                        outgoing_sender
                            .send((remote_addr, plain))
                            .map_err(|_| ChannelerError::SendOutgoingError)
                            .and_then(move |_| Ok(handshake_res))
                    })
            })
            .and_then(move |handshake_res| {
                channel_pool.borrow_mut().add_channel(remote_addr, handshake_res);
                Ok(())
            });

        self.handle.spawn(process_exchange_passive_task.map_err(|e| {
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
            if let Some(outgoing_msg) = self.outgoing_buffered.take() {
                try_ready!(self.start_send_outgoing(outgoing_msg));
            }

            if let Some(networker_msg) = self.networker_buffered.take() {
                try_ready!(self.start_send_networker(networker_msg));
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


// =============================== Helpers =============================

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
