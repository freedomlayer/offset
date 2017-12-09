use std::rc::Rc;
use std::{io, mem};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::collections::HashMap;
use std::collections::VecDeque;

use capnp::serialize_packed;

use futures::sync::mpsc;
use futures::stream::{SplitSink, SplitStream};
use futures::{Async, Future, Poll, Stream, Sink, AsyncSink};

use tokio_core::reactor::Handle;
use tokio_core::net::{TcpStream, TcpStreamNew};

use tokio_io::AsyncRead;
use tokio_io::codec::Framed;

use bytes::{Bytes, BytesMut};
use ring::rand::SystemRandom;

use crypto::rand_values::RandValue;
use crypto::symmetric_enc::{SymmetricKey, Encryptor, Decryptor, EncNonceCounter, SymmetricEncError};
use crypto::identity::{PublicKey, Signature};
use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use security_module::security_module_client::{SecurityModuleClient, SecurityModuleClientError};

use schema::SchemaError;
use schema::channeler_capnp::{self, init_channel, exchange};
use schema::{read_custom_u_int128, write_custom_u_int128,
             read_custom_u_int256, write_custom_u_int256,
             read_custom_u_int512, write_custom_u_int512,
             serialize_message,   read_decrypted_message};

use super::{ToChannel, ChannelerNeighbor};
use super::prefix_frame_codec::{PrefixFrameCodec, PrefixFrameCodecError};

#[derive(Debug)]
pub enum ChannelError {
    Io(io::Error),
    Schema(SchemaError),
    Capnp(::capnp::Error),
    Codec(PrefixFrameCodecError),
    SecurityModule(SecurityModuleClientError),
    EncryptError(SymmetricEncError),
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

    neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>,

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

#[derive(PartialEq, Eq)]
enum Role {
    Initiator,
    Responder,
}

enum ChannelNewState {
    // Connecting to the given address
    Connecting(TcpStreamNew),

    // Waiting public key from SecurityModuleClient
    WaitingPublicKey(Box<Future<Item=PublicKey, Error=SecurityModuleClientError>>),

    // Trying to send serialized InitChannel message
    SendingInit(Option<Bytes>),

    // Waiting the InitChannel message from neighbor
    WaitingInit,

    // Waiting signature from SecurityModuleClient
    WaitingSignature(Box<Future<Item=Signature, Error=SecurityModuleClientError>>),

    // Trying to send serialized Exchange message
    SendingExchange(Option<Bytes>),

    // Waiting the Exchange message from neighbor
    WaitingExchange,

    // The handshake finished, we need this state for the limitation of lifetime module
    Finished((SymmetricKey, SymmetricKey)),
    Empty,
}

impl Channel {
    /// Create a new channel connected to the specified neighbor.
    pub fn connect(addr: &SocketAddr, handle: &Handle,
                   neighbor_public_key: &PublicKey,
                   neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>,
                   sm_client: &SecurityModuleClient) -> ChannelNew {
        ChannelNew {
            state:     ChannelNewState::Connecting(TcpStream::connect(addr, handle)),
            role:      Role::Initiator,
            rng:       SystemRandom::new(),
            sm_client: sm_client.clone(),
            neighbors: neighbors,

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

    /// Create a new channel from a incoming socket.
    pub fn from_socket(socket: TcpStream, _handle: &Handle,
                       neighbors: Rc<RefCell<HashMap<PublicKey, ChannelerNeighbor>>>,
                       sm_client: &SecurityModuleClient) -> ChannelNew {
        let (tx, rx) = socket.framed(PrefixFrameCodec::new()).split();

        let public_key_fut = sm_client.request_public_key();

        ChannelNew {
            state:     ChannelNewState::WaitingPublicKey(Box::new(public_key_fut)),
            role:      Role::Responder,
            rng:       SystemRandom::new(),
            sm_client: sm_client.clone(),
            neighbors: neighbors,

            neighbor_public_key: None,
            sent_rand_value:     None,
            recv_rand_value:     None,
            dh_private_key:      None,
            dh_public_key:       None,
            dh_key_salt:         None,
            sender:              Some(tx),
            receiver:            Some(rx),
        }
    }
}

impl ChannelNew {
    /// Create a InitChannel message
    ///
    /// Create a new InitChannel message, return the serialized message if succeed.
    #[inline]
    fn create_init_channel_message(&mut self, public_key: PublicKey) -> Result<Bytes, ChannelError> {
        let mut message = ::capnp::message::Builder::new_default();

        {
            let mut init_channel_msg = message.init_root::<init_channel::Builder>();

            // Set neighborPublicKey
            {
                let mut neighbor_public_key =
                    init_channel_msg.borrow().init_neighbor_public_key();
                let public_key_bytes = Bytes::from(public_key.as_bytes());

                write_custom_u_int256(&mut neighbor_public_key, &public_key_bytes)?;
            }
            // Set channelRandValue
            {
                let mut channel_rand_value =
                    init_channel_msg.borrow().init_channel_rand_value();
                let rand_value = RandValue::new(&self.rng);
                let rand_value_bytes = Bytes::from(rand_value.as_bytes());

                self.sent_rand_value = Some(rand_value);  // NOTICE: Do not forget this line

                write_custom_u_int128(&mut channel_rand_value, &rand_value_bytes)?;
            }
        }

        let mut serialized_msg = Vec::new();
        serialize_packed::write_message(&mut serialized_msg, &message)?;

        Ok(Bytes::from(serialized_msg))
    }

    /// Create a Exchange message
    ///
    /// Create a new Exchange message, return the serialized message if succeed.
    #[inline]
    fn create_exchange_message(&self, signature: Signature) -> Result<Bytes, ChannelError> {
        let mut message = ::capnp::message::Builder::new_default();

        {
            let mut exchange_msg = message.init_root::<exchange::Builder>();

            // Set the commPublicKey
            {
                let mut comm_public_key = exchange_msg.borrow().init_comm_public_key();
                let comm_public_key_bytes = match self.dh_public_key {
                    None => unreachable!("can not found commPublicKey"),
                    Some(ref key) => Bytes::from(key.as_bytes()),
                };

                write_custom_u_int256(&mut comm_public_key, &comm_public_key_bytes)?;
            }
            // Set the keySalt
            {
                let mut key_salt = exchange_msg.borrow().init_key_salt();
                let key_salt_bytes = match self.dh_key_salt {
                    None => unreachable!("can not found keySalt"),
                    Some(ref key_salt) => Bytes::from(key_salt.as_bytes()),
                };
                write_custom_u_int256(&mut key_salt, &key_salt_bytes)?;
            }
            // Set the signature
            {
                let mut ex_signature = exchange_msg.borrow().init_signature();
                let signature_bytes = Bytes::from(signature.as_bytes());
                write_custom_u_int512(&mut ex_signature, &signature_bytes)?;
            }
        }

        let mut serialized_msg = Vec::new();
        serialize_packed::write_message(&mut serialized_msg, &message)?;

        Ok(Bytes::from(serialized_msg))
    }

    /// Read InitChannel message from Bytes.
    ///
    /// Read InitChannel message from Bytes, return the `neighborPublicKey` and
    /// `channelRandValue` if succeed.
    #[inline]
    fn read_init_channel_message(buffer: Bytes) -> Result<(PublicKey, RandValue), ChannelError> {
        let mut buffer = io::Cursor::new(buffer);
        let message_rdr = serialize_packed::read_message(&mut buffer, ::capnp::message::ReaderOptions::new())?;

        let init_channel_msg = message_rdr.get_root::<init_channel::Reader>()?;

        // Read the neighborPublicKey
        let neighbor_public_key = init_channel_msg.get_neighbor_public_key()?;
        let mut public_key_bytes = BytesMut::with_capacity(32);
        read_custom_u_int256(&mut public_key_bytes, &neighbor_public_key)?;

        // Read the channelRandValue
        let channel_rand_value = init_channel_msg.get_channel_rand_value()?;
        let mut rand_value_bytes = BytesMut::with_capacity(16);
        read_custom_u_int128(&mut rand_value_bytes, &channel_rand_value)?;

        // FIXME: Remove unwrap usage
        let public_key = PublicKey::from_bytes(&public_key_bytes).unwrap();
        let rand_value = RandValue::from_bytes(&rand_value_bytes).unwrap();

        Ok((public_key, rand_value))
    }

    /// Read Exchange message from Bytes.
    ///
    /// Read Exchange message from Bytes, return the `commPublicKey`, `keySalt` and
    /// `signature` if succeed.
    #[inline]
    fn read_exchange_message(buffer: Bytes) -> Result<(DhPublicKey, Salt, Signature), ChannelError> {
        let mut buffer = io::Cursor::new(buffer);
        let message_rdr = serialize_packed::read_message(&mut buffer, ::capnp::message::ReaderOptions::new())?;

        let exchange_msg = message_rdr.get_root::<exchange::Reader>()?;

        // Read the commPublicKey
        let comm_public_key = exchange_msg.get_comm_public_key()?;
        let mut comm_public_key_bytes = BytesMut::with_capacity(32);
        read_custom_u_int256(&mut comm_public_key_bytes, &comm_public_key)?;

        // Read the keySalt
        let key_salt = exchange_msg.get_key_salt()?;
        let mut key_salt_bytes = BytesMut::with_capacity(32);
        read_custom_u_int256(&mut key_salt_bytes, &key_salt)?;

        // Read the signature
        let signature = exchange_msg.get_signature()?;
        let mut signature_bytes = BytesMut::with_capacity(64);
        read_custom_u_int512(&mut signature_bytes, &signature)?;

        // FIXME: Remove unwrap usage
        let public_key = DhPublicKey::from_bytes(&comm_public_key_bytes).unwrap();
        let key_salt   = Salt::from_bytes(&key_salt_bytes).unwrap();
        let signature  = Signature::from_bytes(&signature_bytes).unwrap();

        Ok((public_key, key_salt, signature))
    }
}

impl Future for ChannelNew {
    type Item = Channel;
    type Error = ChannelError;

    fn poll(&mut self) -> Poll<Channel, ChannelError> {
        match mem::replace(&mut self.state, ChannelNewState::Empty) {
            ChannelNewState::Connecting(mut stream_new) => {
                match stream_new.poll()? {
                    Async::Ready(tcp_stream) => {
                        trace!("ChannelNewState::Connecting [Ready]");
                        let (tx, rx)  = tcp_stream.framed(PrefixFrameCodec::new()).split();
                        self.sender   = Some(tx);
                        self.receiver = Some(rx);

                        let public_key_fut = self.sm_client.request_public_key();
                        self.state = ChannelNewState::WaitingPublicKey(Box::new(public_key_fut));

                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::Connecting [NotReady]");
                        self.state = ChannelNewState::Connecting(stream_new);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::WaitingPublicKey(mut boxed_public_key_fut) => {
                match boxed_public_key_fut.poll()? {
                    Async::Ready(public_key) => {
                        trace!("ChannelNewState::WaitingPublicKey [Ready]");

                        let serialized_msg = self.create_init_channel_message(public_key)?;
                        self.state = ChannelNewState::SendingInit(Some(serialized_msg));

                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::WaitingPublicKey [NotReady]");

                        self.state = ChannelNewState::WaitingPublicKey(boxed_public_key_fut);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::SendingInit(serialize_msg) => {
                let mut poll_forward = false;

                match self.sender {
                    None => unreachable!(),
                    Some(ref mut sender) => {
                        if let Some(msg) = serialize_msg {
                            match sender.start_send(msg)? {
                                AsyncSink::Ready => {
                                    self.state = ChannelNewState::SendingInit(None);
                                    poll_forward = true;
                                }
                                AsyncSink::NotReady(msg) => {
                                    self.state = ChannelNewState::SendingInit(Some(msg));
                                }
                            }
                        } else {
                            match sender.poll_complete()? {
                                Async::NotReady => {
                                    trace!("ChannelNewState::SendingInit [NotReady]");
                                    self.state = ChannelNewState::SendingInit(None);
                                }
                                Async::Ready(_) => {
                                    trace!("ChannelNewState::SendingInit [Ready]");
                                    self.state = ChannelNewState::WaitingInit;
                                    poll_forward = true;
                                }
                            }
                        }
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::WaitingInit => {
                let mut poll_forward = false;

                match self.receiver {
                    None => unreachable!(),
                    Some(ref mut receiver) => {
                        if let Async::Ready(Some(buf)) = receiver.poll()? {
                            trace!("ChannelNewState::WaitingInit [Ready]");

                            let (public_key, recv_rand_value) =
                                ChannelNew::read_init_channel_message(buf)?;

                            // Verify the received neighborPublicKey
                            if let Some(ref key) = self.neighbor_public_key {
                                if key.as_ref() != public_key.as_ref() {
                                    error!("neighbor public key not match");
                                    return Err(ChannelError::Closed("neighbor public key not match"))
                                }
                            } else {
                                match self.neighbors.borrow().get(&public_key) {
                                    None => {
                                        error!("Unexpected neighbor");
                                        return Err(ChannelError::Closed("Unexpected neighbor"))
                                    }
                                    Some(ref neighbor) => {
                                        if neighbor.info.neighbor_address.socket_addr.is_some() {
                                            error!("Can not initiate a channel by this neighbor");
                                            return Err(ChannelError::Closed("Can not initiate \
                                                 a channel by this neighbor"))
                                        }
                                    }
                                }
                                self.neighbor_public_key = Some(public_key);
                            }

                            // Generate ephemeral DH private key
                            let dh_key_salt    = Salt::new(&self.rng);
                            let dh_private_key = DhPrivateKey::new(&self.rng);
                            let dh_public_key  = dh_private_key.compute_public_key();

                            // message = (channelRandValue + commPublicKey + keySalt)
                            let mut message = Vec::with_capacity(1024);
                            message.extend_from_slice(recv_rand_value.as_bytes());
                            message.extend_from_slice(dh_public_key.as_bytes());
                            message.extend_from_slice(dh_key_salt.as_bytes());

                            // Request signature from SecurityModuleClient
                            let signature_fut = self.sm_client.request_sign(message);

                            // Keep these values
                            self.dh_key_salt     = Some(dh_key_salt);
                            self.dh_public_key   = Some(dh_public_key);
                            self.dh_private_key  = Some(dh_private_key);
                            self.recv_rand_value = Some(recv_rand_value);

                            self.state = ChannelNewState::WaitingSignature(Box::new(signature_fut));
                            poll_forward = true;
                        } else {
                            trace!("ChannelNewState::WaitingInit [Not Ready]");
                            self.state = ChannelNewState::WaitingInit;
                        }
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::WaitingSignature(mut boxed_signature_fut) => {
                match boxed_signature_fut.poll()? {
                    Async::Ready(signature) => {
                        trace!("ChannelNewState::WaitingSignature [Ready]");

                        let serialized_msg = self.create_exchange_message(signature)?;
                        self.state = ChannelNewState::SendingExchange(Some(serialized_msg));

                        self.poll()
                    }
                    Async::NotReady => {
                        trace!("ChannelNewState::WaitingSignature [Not Ready]");

                        self.state = ChannelNewState::WaitingSignature(boxed_signature_fut);
                        Ok(Async::NotReady)
                    }
                }
            }
            ChannelNewState::SendingExchange(serialize_msg) => {
                let mut poll_forward = false;

                match self.sender {
                    None => unreachable!(),
                    Some(ref mut sender) => {
                        if let Some(msg) = serialize_msg {
                            match sender.start_send(msg)? {
                                AsyncSink::Ready => {
                                    self.state = ChannelNewState::SendingExchange(None);
                                    poll_forward = true;
                                }
                                AsyncSink::NotReady(msg) => {
                                    self.state = ChannelNewState::SendingExchange(Some(msg));
                                }
                            }
                        } else {
                            match sender.poll_complete()? {
                                Async::NotReady => {
                                    trace!("ChannelNewState::SendingExchange [NotReady]");
                                    self.state = ChannelNewState::SendingExchange(None);
                                }
                                Async::Ready(_) => {
                                    trace!("ChannelNewState::SendingExchange [Ready]");
                                    self.state = ChannelNewState::WaitingExchange;
                                    poll_forward = true;
                                }
                            }
                        }
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::WaitingExchange => {
                let mut poll_forward = false;

                match self.receiver {
                    None => unreachable!(),
                    Some(ref mut receiver) => {
                        match receiver.poll()? {
                            Async::Ready(buf) => {
                                if let Some(buf) = buf {
                                    trace!("ChannelNewState::WaitingExchange [Ready]");

                                    let (public_key, recv_key_salt, signature) =
                                        ChannelNew::read_exchange_message(buf)?;

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
                                        Some(ref key) => key,
                                    };

                                    let sent_key_salt = match self.dh_key_salt {
                                        None => unreachable!(),
                                        Some(ref salt) => salt,
                                    };

                                    // Verify this message was signed by the neighbor with its private key
                                    if ::crypto::identity::verify_signature(&message, neighbor_public_key, &signature) {
                                        let dh_private_key = mem::replace(&mut self.dh_private_key, None).unwrap();
                                        // received public key + sent key salt -> symmetric key for sending data
                                        let key_send = dh_private_key.derive_symmetric_key(&public_key, &sent_key_salt);
                                        // received public key + received key salt -> symmetric key for receiving data
                                        let key_recv = dh_private_key.derive_symmetric_key(&public_key, &recv_key_salt);

                                        self.state = ChannelNewState::Finished((key_send, key_recv));
                                        poll_forward = true;
                                    } else {
                                        error!("invalid signature");
                                        return Err(ChannelError::Closed("invalid signature"))
                                    }
                                } else {
                                    trace!("connection lost");
                                    return Err(ChannelError::Closed("connection lost"))
                                }
                            }
                            Async::NotReady => {
                                trace!("ChannelNewState::WaitingExchange [NotReady]");
                                self.state = ChannelNewState::WaitingExchange;
                            }
                        }
                    }
                }

                if poll_forward {
                    self.poll()
                } else {
                    Ok(Async::NotReady)
                }
            }
            ChannelNewState::Finished((key_send, key_recv)) => {
                trace!("ChannelNewState::Finished");

                let (tx, rx) = mpsc::channel::<ToChannel>(0);

                let mut neighbors = self.neighbors.borrow_mut();
                let neighbor_public_key = match self.neighbor_public_key {
                    None => unreachable!(),
                    Some(ref key) => key,
                };

                match neighbors.get_mut(neighbor_public_key) {
                    None => unreachable!("no such neighbor"),
                    Some(neighbor) => {
                        neighbor.channels.push(tx);
                        if self.role == Role::Initiator {
                            // FIXME: Uncomment this before submit
                            // neighbor.num_pending_out_conn -= 1;
                        }
                        // FIXME: Check and send notification to Networker here?
                        // If we do so, we need a mpsc::Sender<ChannelerToNetworker> here,
                        // or we can check this when time tick event occurred.
                    }
                };

                Ok(Async::Ready(Channel {
                    receiver: rx,
                    send_encryptor: Encryptor::new(&key_send, EncNonceCounter::new(&mut self.rng)),
                    recv_decryptor: Decryptor::new(&key_recv),
                    send_counter: 0,
                    recv_counter: 0,
                    tcp_sender:   mem::replace(&mut self.sender, None).unwrap(),
                    tcp_receiver: mem::replace(&mut self.receiver, None).unwrap(),
                    transmit_queue: VecDeque::new(),
                }))
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
