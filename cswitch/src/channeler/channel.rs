use std::{io, mem};
use std::net::SocketAddr;
use std::collections::HashMap;
use std::collections::VecDeque;

use capnp::serialize_packed;

use futures::sync::mpsc;
use futures::stream::{SplitSink, SplitStream};
use futures::{Async, Future, Poll, Stream, Sink, AsyncSink};

use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;

use tokio_io::AsyncRead;
use tokio_io::codec::Framed;

use bytes::{Bytes, BytesMut};
use ring::rand::SystemRandom;

use async_mutex::{AsyncMutex, AsyncMutexError, IoError};
use crypto::rand_values::RandValue;
use crypto::symmetric_enc::{SymmetricKey, Encryptor, Decryptor, EncNonceCounter, SymmetricEncError};
use crypto::identity::{PublicKey, Signature};
use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use inner_messages::{ChannelerToNetworker, ChannelOpened, ChannelClosed};
use security_module::security_module_client::{SecurityModuleClient, SecurityModuleClientError};

use schema::SchemaError;
use schema::channeler_capnp::{self, init_channel, exchange};
use schema::{serialize_init_channel_message, deserialize_init_channel_message,
             serialize_exchange_message, deserialize_exchange_message};
//use schema::{read_custom_u_int128, write_custom_u_int128,
//             read_custom_u_int256, write_custom_u_int256,
//             read_custom_u_int512, write_custom_u_int512,
//             serialize_message,   read_decrypted_message};

use super::{ToChannel, ChannelerNeighbor};
use super::codec::{PrefixFrameCodec, PrefixFrameCodecError};

#[derive(Debug)]
pub enum ChannelError {
    Io(io::Error),
    AsyncMutexIo(IoError),
    Schema(SchemaError),
    Capnp(::capnp::Error),
    Codec(PrefixFrameCodecError),
    SecurityModule(SecurityModuleClientError),
    EncryptError(SymmetricEncError),
    SendToNetworkerFailed,
    Closed(&'static str),
}

impl From<io::Error> for ChannelError {
    #[inline]
    fn from(e: io::Error) -> ChannelError {
        ChannelError::Io(e)
    }
}

impl From<::capnp::Error> for ChannelError {
    #[inline]
    fn from(e: ::capnp::Error) -> ChannelError {
        ChannelError::Capnp(e)
    }
}

impl From<PrefixFrameCodecError> for ChannelError {
    #[inline]
    fn from(e: PrefixFrameCodecError) -> ChannelError {
        ChannelError::Codec(e)
    }
}

impl From<SecurityModuleClientError> for ChannelError {
    #[inline]
    fn from(e: SecurityModuleClientError) -> ChannelError {
        ChannelError::SecurityModule(e)
    }
}

impl From<SchemaError> for ChannelError {
    #[inline]
    fn from(e: SchemaError) -> ChannelError {
        ChannelError::Schema(e)
    }
}

impl From<SymmetricEncError> for ChannelError {
    fn from(e: SymmetricEncError) -> ChannelError {
        ChannelError::EncryptError(e)
    }
}

impl From<AsyncMutexError<ChannelError>> for ChannelError {
    fn from(e: AsyncMutexError<ChannelError>) -> ChannelError {
        match e {
            AsyncMutexError::IoError(io_e) => ChannelError::AsyncMutexIo(io_e),
            AsyncMutexError::FuncError(func_e) => func_e.into(),
        }
    }
}

/// The channel used to communicate to neighbors.
pub struct Channel {
    receiver: mpsc::Receiver<ToChannel>,

    tcp_sender:   SplitSink<Framed<TcpStream, PrefixFrameCodec>>,
    tcp_receiver: SplitStream<Framed<TcpStream, PrefixFrameCodec>>,

    send_counter: u64,
    recv_counter: u64,

    send_encryptor: Encryptor,
    recv_decryptor: Decryptor,

    transmit_queue: VecDeque<Bytes>,
}

//impl Future for Channel {
//    type Item = ();
//    type Error = ChannelError;
//
//    fn poll(&mut self) -> Poll<(), Self::Error> {
//        // Check message from others modules
//        match self.receiver.poll().unwrap() {
//            Async::NotReady => (),
//            Async::Ready(message) => {
//                if let Some(message) = message {
//                    match message {
//                        ToChannel::TimeTick => {
//                            // TODO: Try to receive message
//                            // TODO: Send Received notification to Networker
//                        }
//                        ToChannel::SendMessage(msg) => {
//                            // TODO: Schedule a new sending task
//                            // TODO: Report the send result?
//                            let serialized_msg =
//                                serialize_message(self.send_counter, Some(Bytes::from(msg)))?;
//                            let enc_msg = self.send_encryptor.encrypt(&serialized_msg)?;
//
//                            let mut message = ::capnp::message::Builder::new_default();
//
//                            {
//                                let mut msg = message.init_root::<channeler_capnp::message::Builder>();
//
//                                let mut content = msg.init_content(enc_msg.len() as u32);
//                                content.copy_from_slice(&enc_msg[..]);
//                            }
//
//                            let mut final_msg = Vec::new();
//                            serialize_packed::write_message(&mut final_msg, &message);
//                            self.transmit_queue.push_back(Bytes::from(final_msg));
//
//                            self.send_counter += 1;
//                        }
//                    }
//                } else {
//                    // TODO: Update the neighbor table
//                    // TODO: Send notification to Networker?
//                    return Err(ChannelError::Closed("connection lost"));
//                }
//            }
//        };
//
//        match self.transmit_queue.pop_front() {
//            None => (),
//            Some(msg) => {
//                match self.tcp_sender.start_send(msg)? {
//                    AsyncSink::Ready => {
//                    }
//                    AsyncSink::NotReady(msg) => {
//                        self.transmit_queue.push_front(msg);
//                    }
//                }
//            }
//        };
//    }
//}

pub struct ChannelNew {
    role: Role,
    state: ChannelNewState,

    // Utils used in performing exchange
    rng:       SystemRandom,
    sm_client: SecurityModuleClient,

    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,

    // The public key of neighbor
    neighbor_public_key: Option<PublicKey>,

    sent_rand_value: Option<RandValue>,
    recv_rand_value: Option<RandValue>,

    // The parts used to perform DH exchange
    dh_key_salt:    Option<Salt>,
    dh_public_key:  Option<DhPublicKey>,
    dh_private_key: Option<DhPrivateKey>,

    sender:   Option<SplitSink<Framed<TcpStream, PrefixFrameCodec>>>,
    receiver: Option<SplitStream<Framed<TcpStream, PrefixFrameCodec>>>,
}

#[derive(Clone, PartialEq, Eq)]
enum Role {
    Initiator,
    Responder,
}

enum ChannelNewState {
    // Prepare a TCP connection used in Channel, at this stage, we should finish:
    //
    // 1. Establish a TCP connection to the remote
    // 2. Increase the `num_pending_out_conn` for the given neighbor
    PrepareTcp(Box<Future<Item=TcpStream, Error=AsyncMutexError<ChannelError>>>),

    // Prepare a serialized InitChannel message, at this stage, we should finish:
    //
    // 1. Request the public key from SecurityModuleClient
    // 2. Create and serialize the InitChannel message to send
    PrepareInit(Box<Future<Item=Bytes, Error=ChannelError>>),

    // Trying to send serialized InitChannel message
    SendInit(Option<Bytes>),

    // Waiting the InitChannel message from neighbor
    WaitInit,

    VerifyNeighbor {
        public_key: PublicKey,
        recv_rand_value: RandValue,
        verify_neighbor_fut: Box<Future<Item=(), Error=AsyncMutexError<ChannelError>>>
    },

    // Prepare a serialized Exchange message, at this stage, we should finish:
    //
    // 1. Request the signature from SecurityModuleClient
    // 2. Create and serialize the Exchange message to send
    PrepareExchange(Box<Future<Item=Bytes, Error=ChannelError>>),

    // Trying to send serialized Exchange message
    SendExchange(Option<Bytes>),

    // Waiting the Exchange message from neighbor
    WaitExchange,

    // The final stage to build a channel, as this stage, we should finish:
    //
    // 1. Decrease the `num_pending_out_conn` for the given neighbor
    // 2. Send a `ChannelOpened` notification to Networker if needed
    // 3. Add a new mpsc::Sender<ToChannel> to the `channel_senders`
    FinalStage(Box<Future<Item=(SymmetricKey,SymmetricKey, mpsc::Receiver<ToChannel>),
        Error=AsyncMutexError<ChannelError>>>),

    Empty,
}

impl Channel {
    /// Create a new channel connected to the specified neighbor.
    pub fn connect(addr: &SocketAddr, handle: &Handle,
                   neighbor_public_key: &PublicKey,
                   neighbors: &AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
                   networker_sender: &mpsc::Sender<ChannelerToNetworker>,
                   sm_client: &SecurityModuleClient) -> ChannelNew {
        let neighbors_a = neighbors.clone();
        let neighbor_public_key_a = neighbor_public_key.clone();
        let prepare_fut = TcpStream::connect(addr, handle)
            .map_err(|e| ChannelError::Io(e).into())
            .and_then(move |tcp_stream| {
                neighbors_a.clone().acquire(move |mut neighbors| {
                    match neighbors.get_mut(&neighbor_public_key_a) {
                        None => return Err(ChannelError::Closed("Unknown neighbors")),
                        Some(neighbor) => {
                            neighbor.num_pending_out_conn += 1;
                        }
                    }
                    Ok((neighbors, tcp_stream))
                })
            });
        ChannelNew {
            state:     ChannelNewState::PrepareTcp(Box::new(prepare_fut)),
            role:      Role::Initiator,
            rng:       SystemRandom::new(),
            sm_client: sm_client.clone(),
            neighbors: neighbors.clone(),
            networker_sender: networker_sender.clone(),

            neighbor_public_key: Some(neighbor_public_key.clone()),
            sent_rand_value:     None,
            recv_rand_value:     None,
            dh_private_key:      None,
            dh_public_key:       None,
            dh_key_salt:         None,
            sender:              None,
            receiver:            None,
        }
    }

    // Create a new channel from a incoming socket.
    pub fn from_socket(socket: TcpStream, _handle: &Handle,
                       neighbors: &AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
                       networker_sender: &mpsc::Sender<ChannelerToNetworker>,
                       sm_client: &SecurityModuleClient) -> ChannelNew {
        let (tx, rx) = socket.framed(PrefixFrameCodec::new()).split();

        let rng = SystemRandom::new();
        let rand_value = RandValue::new(&rng);
        let sent_rand_value = rand_value.clone();

        let prepare_init_fut = sm_client.request_public_key()
            .map_err(|e| e.into())
            .and_then(move |public_key| {
                serialize_init_channel_message(rand_value, public_key)
                    .map_err(|e| e.into())
            });

        ChannelNew {
            state:     ChannelNewState::PrepareInit(Box::new(prepare_init_fut)),
            role:      Role::Responder,
            rng:       rng,
            sm_client: sm_client.clone(),
            neighbors: neighbors.clone(),
            networker_sender: networker_sender.clone(),

            neighbor_public_key: None,
            sent_rand_value:     Some(sent_rand_value),
            recv_rand_value:     None,
            dh_private_key:      None,
            dh_public_key:       None,
            dh_key_salt:         None,
            sender:              Some(tx),
            receiver:            Some(rx),
        }
    }
}

impl Future for ChannelNew {
    type Item = Channel;
    type Error = ChannelError;

    fn poll(&mut self) -> Poll<Channel, ChannelError> {
        match mem::replace(&mut self.state, ChannelNewState::Empty) {
            ChannelNewState::PrepareTcp(mut prepare_tcp_fut) => {
                match prepare_tcp_fut.poll()? {
                    Async::Ready(tcp_stream) => {
                        trace!("ChannelNewState::PrepareTcp [Ready]");
                        let (tx, rx)  = tcp_stream.framed(PrefixFrameCodec::new()).split();
                        self.sender   = Some(tx);
                        self.receiver = Some(rx);

                        let rand_value = RandValue::new(&self.rng);
                        self.sent_rand_value = Some(rand_value.clone());

                        let prepare_init_fut = self.sm_client.request_public_key()
                            .map_err(|e| e.into())
                            .and_then(move |public_key| {
                                serialize_init_channel_message(rand_value, public_key)
                                    .map_err(|e| e.into())
                            });

                        self.state = ChannelNewState::PrepareInit(Box::new(prepare_init_fut));
                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::PrepareTcp [NotReady]");

                        self.state = ChannelNewState::PrepareTcp(prepare_tcp_fut);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::PrepareInit(mut prepare_init_fut) => {
                match prepare_init_fut.poll()? {
                    Async::Ready(serialized_msg) => {
                        trace!("ChannelNewState::PrepareInit [Ready]");

                        self.state = ChannelNewState::SendInit(Some(serialized_msg));
                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::PrepareInit [NotReady]");

                        self.state = ChannelNewState::PrepareInit(prepare_init_fut);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::SendInit(serialized_msg) => {
                debug_assert!(self.sender.is_some());

                let mut go_forward = false;

                if let Some(ref mut sender) = self.sender {
                    if let Some(msg) = serialized_msg {
                        match sender.start_send(msg)? {
                            AsyncSink::Ready => {
                                self.state = ChannelNewState::SendInit(None);
                                go_forward = true;
                            }
                            AsyncSink::NotReady(msg) => {
                                self.state = ChannelNewState::SendInit(Some(msg));
                            }
                        }
                    } else {
                        match sender.poll_complete()? {
                            Async::NotReady => {
                                trace!("ChannelNewState::SendingInit [NotReady]");

                                self.state = ChannelNewState::SendInit(None);
                            }
                            Async::Ready(_) => {
                                trace!("ChannelNewState::SendingInit [Ready]");

                                self.state = ChannelNewState::WaitInit;
                                go_forward = true;
                            }
                        }
                    }
                }

                if go_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::WaitInit => {
                debug_assert!(self.receiver.is_some());

                let mut poll_forward = false;

                if let Some(ref mut receiver) = self.receiver {
                    if let Async::Ready(Some(buf)) = receiver.poll()? {
                        trace!("ChannelNewState::WaitInit [Ready]");

                        let (public_key, recv_rand_value) = deserialize_init_channel_message(buf)?;

                        let public_key_a = public_key.clone();

                        let neighbor_public_key = self.neighbor_public_key.clone();

                        let verify_neighbor_fut = self.neighbors.acquire(move |neighbors| {
                            if let Some(key) = neighbor_public_key {
                                if key.as_ref() != public_key_a.as_ref() {
                                    return Err(ChannelError::Closed("neighbor public key not match"));
                                }
                            } else {
                                match neighbors.get(&public_key_a) {
                                    None => return Err(ChannelError::Closed("Unknown neighbor")),
                                    Some(ref neighbor) => {
                                        if neighbor.info.neighbor_address.socket_addr.is_some() {
                                            return Err(ChannelError::Closed("Not allow"));
                                        }
                                    }
                                }
                            }

                            Ok((neighbors, ()))
                        });

                        self.state = ChannelNewState::VerifyNeighbor {
                            public_key,
                            recv_rand_value,
                            verify_neighbor_fut: Box::new(verify_neighbor_fut),
                        };
                        poll_forward = true;
                    } else {
                        trace!("ChannelNewState::WaitingInit [Not Ready]");

                        self.state = ChannelNewState::WaitInit;
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::VerifyNeighbor{ public_key, recv_rand_value, mut verify_neighbor_fut } => {
                match verify_neighbor_fut.poll()? {
                    Async::NotReady => Ok(Async::NotReady),
                    Async::Ready(_) => {
                        self.neighbor_public_key = Some(public_key);

                        // Generate ephemeral DH private key
                        let dh_key_salt    = Salt::new(&self.rng);
                        let dh_private_key = DhPrivateKey::new(&self.rng);
                        let dh_public_key  = dh_private_key.compute_public_key();

                        // message = (channelRandValue + commPublicKey + keySalt)
                        let mut message = Vec::with_capacity(1024);
                        message.extend_from_slice(recv_rand_value.as_bytes());
                        message.extend_from_slice(dh_public_key.as_bytes());
                        message.extend_from_slice(dh_key_salt.as_bytes());

                        // Keep these values
                        self.dh_key_salt     = Some(dh_key_salt.clone());
                        self.dh_public_key   = Some(dh_public_key.clone());
                        self.dh_private_key  = Some(dh_private_key);
                        self.recv_rand_value = Some(recv_rand_value);

                        let prepare_exchange_fut = self.sm_client.request_sign(message)
                            .map_err(|e| e.into())
                            .and_then(move |signature| {
                                serialize_exchange_message(dh_public_key, dh_key_salt, signature)
                                    .map_err(|e| e.into())
                            });

                        self.state = ChannelNewState::PrepareExchange(Box::new(prepare_exchange_fut));
                        self.poll()
                    }
                }
            }
            ChannelNewState::PrepareExchange(mut prepare_exchange_fut) => {
                match prepare_exchange_fut.poll()? {
                    Async::Ready(serialized_msg) => {
                        trace!("ChannelNewState::PrepareExchange [Ready]");

                        self.state = ChannelNewState::SendExchange(Some(serialized_msg));
                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::PrepareExchange [Not Ready]");

                        self.state = ChannelNewState::PrepareExchange(prepare_exchange_fut);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::SendExchange(serialize_msg) => {
                debug_assert!(self.sender.is_some());

                let mut go_forward = false;

                if let Some(ref mut sender) = self.sender {
                    if let Some(msg) = serialize_msg {
                        match sender.start_send(msg)? {
                            AsyncSink::Ready => {
                                self.state = ChannelNewState::SendExchange(None);
                                go_forward = true;
                            }
                            AsyncSink::NotReady(msg) => {
                                self.state = ChannelNewState::SendExchange(Some(msg));
                            }
                        }
                    } else {
                        match sender.poll_complete()? {
                            Async::NotReady => {
                                trace!("ChannelNewState::SendingExchange [NotReady]");

                                self.state = ChannelNewState::SendExchange(None);
                            }
                            Async::Ready(_) => {
                                trace!("ChannelNewState::SendingExchange [Ready]");

                                self.state = ChannelNewState::WaitExchange;
                                go_forward = true;
                            }
                        }
                    }
                }


                if go_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::WaitExchange => {
                debug_assert!(self.receiver.is_some());

                let mut poll_forward = false;

                if let Some(ref mut receiver) = self.receiver {
                    match receiver.poll()? {
                        Async::Ready(buffer) => {
                            trace!("ChannelNewState::WaitExchange [Ready]");

                            if buffer.is_none() {
                                error!("Connection lost");
                                return Err(ChannelError::Closed("Connection lost"));
                            }

                            let buffer = buffer.unwrap();

                            let (public_key, recv_key_salt, signature) =
                                deserialize_exchange_message(buffer)?;

                            let channel_rand_value = match self.sent_rand_value {
                                None => unreachable!(),
                                Some(ref rand_value) => rand_value,
                            };

                            // message = (channelRandValue + commPublicKey + keySalt)
                            let mut message = Vec::new();
                            message.extend_from_slice(channel_rand_value.as_bytes());
                            message.extend_from_slice(public_key.as_bytes());
                            message.extend_from_slice(recv_key_salt.as_bytes());

                            let neighbor_public_key = match self.neighbor_public_key {
                                None => unreachable!(),
                                Some(ref key) => key.clone(),
                            };

                            let sent_key_salt = match self.dh_key_salt {
                                None => unreachable!(),
                                Some(ref salt) => salt.clone(),
                            };

                            // Verify this message was signed by the neighbor with its private key
                            if ::crypto::identity::verify_signature(&message, &neighbor_public_key, &signature) {
                                let dh_private_key = mem::replace(&mut self.dh_private_key, None).unwrap();
                                // received public key + sent_key_salt -> symmetric key for sending data
                                let key_send = dh_private_key.derive_symmetric_key(&public_key, &sent_key_salt);
                                // received public key + recv_key_salt -> symmetric key for receiving data
                                let key_recv = dh_private_key.derive_symmetric_key(&public_key, &recv_key_salt);

                                let role = self.role.clone();
                                let mut networker_sender = self.networker_sender.clone();

                                let final_stage_fut = self.neighbors.acquire(move |mut neighbors| {
                                    let (tx, rx) = mpsc::channel::<ToChannel>(0);

                                    match neighbors.get_mut(&neighbor_public_key) {
                                        None => return Err(ChannelError::Closed("Unknown neighbor")),
                                        Some(neighbor) => {
                                            if role == Role::Initiator {
                                                neighbor.num_pending_out_conn -= 1;
                                            }

                                            if neighbor.channels.is_empty() {
                                                let msg = ChannelerToNetworker::ChannelOpened(
                                                    ChannelOpened {
                                                            remote_public_key: neighbor_public_key,
                                                            locally_initialized: role == Role::Initiator,
                                                    });

                                                match networker_sender.try_send(msg) {
                                                    Err(_) => {
                                                        error!("Failed to send ChannelOpened message to networker");
                                                        return Err(ChannelError::SendToNetworkerFailed);
                                                    },
                                                    _ => (),
                                                }
                                            }
                                            neighbor.channels.push(tx);
                                        }
                                    }

                                    Ok((neighbors, (key_send, key_recv, rx)))
                                });

                                self.state = ChannelNewState::FinalStage(Box::new(final_stage_fut));
                                poll_forward = true;
                            } else {
                                error!("Invalid signature");
                                return Err(ChannelError::Closed("invalid signature"));
                            }
                        }
                        Async::NotReady => {
                            trace!("ChannelNewState::WaitExchange [NotReady]");

                            self.state = ChannelNewState::WaitExchange;
                        }
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::FinalStage(mut final_stage_fut) => {
                match final_stage_fut.poll()? {
                    Async::NotReady => Ok(Async::NotReady),
                    Async::Ready((key_send, key_recv, channel_receiver)) => {
                        Ok(Async::Ready(Channel {
                            receiver: channel_receiver,
                            send_encryptor: Encryptor::new(&key_send, EncNonceCounter::new(&mut self.rng)),
                            recv_decryptor: Decryptor::new(&key_recv),
                            send_counter: 0,
                            recv_counter: 0,
                            tcp_sender: mem::replace(&mut self.sender, None).unwrap(),
                            tcp_receiver: mem::replace(&mut self.receiver, None).unwrap(),
                            transmit_queue: VecDeque::new(),

                        }))
                    }
                }
            }
            ChannelNewState::Empty => unreachable!("can't poll twice"),
        }
    }
}

//pub fn create_channel(handle: &Handle,
//                      addr: &SocketAddr,
//                      neighbor_public_key: &PublicKey,
//                      sm_client: &SecurityModuleClient)
//    -> impl Future<Item=(), Error=ChannelError> {
//    Channel::connect(handle, addr, neighbor_public_key, sm_client)
//        .and_then(|channel| {
//            let (tx, rx) = mpsc::channel::<ToChannel>();
//
//            rx.for_each(|msg| {
//                match msg {
//                    ToChannel::TimeTick => {
//                        match channel.tcp_receiver.poll()? {
//                            Async::Ready(None) => {
//                                // Send notification to networker
//                            }
//                            Async::Ready(buf) => {
//                                // Parse message
//                            }
//                            Async::NotReady => {
//
//                            }
//                        }
//                    },
//                    ToChannel::SendMessage(_) => {
//                        Ok(())
//                    }
//                }
//
//
//            })
//        })
//
//    // TODO:
//    // Create an mpsc channel that will be used to signal this channel future.
//    // This line should be added to only after
//
//    // Attempt a connection:
//    TcpStream::connect(&socket_addr, handle)
//        .and_then(|stream| {
//            let (sink, stream) = stream.framed(PrefixFrameCodec::new()).split();
//
//            let mut msg_builder = ::capnp::message::Builder::new_default();
//            let init_message    = msg_builder.init_root::<init_channel::Builder>();
//
//
//
//            // TODO: Create Init Channeler message:
//
//            // let mut message = ::capnp::message::Builder::new_default();
//            // let init_channel = message.init_root::<init_channel::Builder>();
//
//
//            Ok(())
//        });
//
//
//
//        // After exchange happened:
//        // neighbor.num_pending_out_conn -= 1;
//        // let (channel_sender, channel_receiver) = mpsc::channel(0);
//        // neighbor.channel_senders.push(AsyncMutex::new(channel_sender));
//
//        // TODO: Binary deserializtion of Channeler to Channeler messages.
//
//    Ok(()).into_future()
//}
