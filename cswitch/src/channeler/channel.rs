extern crate tokio_core;
extern crate tokio_io;
extern crate rand;

use std::io;
use std::mem;
use std::net::SocketAddr;

use capnp::serialize_packed;
use futures::stream::{SplitSink, SplitStream};
use futures::{Async, Future, Poll, IntoFuture, Stream, Sink, AsyncSink};

use self::tokio_core::net::{TcpStream, TcpStreamNew};
use self::tokio_core::reactor::Handle;
use self::tokio_io::codec::Framed;
use self::tokio_io::AsyncRead;

// use ::inner_messages::ChannelerAddress;
use ::crypto::rand_values::RandValue;
use ::crypto::identity::{PublicKey, Signature};
use ::crypto::symmetric_enc::SymmetricKey;
use ::crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use ::schema::channeler_capnp::{init_channel, exchange};
use ::security_module::security_module_client::{SecurityModuleClient,
                                                SecurityModuleClientError};

use self::rand::StdRng;
use ::crypto::test_utils::DummyRandom;

use bytes::{Bytes, BytesMut};
use schema::{read_custom_u_int128, write_custom_u_int128,
             read_custom_u_int256, write_custom_u_int256};

//use super::ToChannel;
use super::prefix_frame_codec::{PrefixFrameCodec, PrefixFrameCodecError};

pub enum ChannelError {
    Io(io::Error),
    Capnp(::capnp::Error),
    Codec(PrefixFrameCodecError),
    SecurityModule(SecurityModuleClientError),
    InvalidSignature,
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

/// The channel used to communicate to neighbors.
pub struct Channel {
    sender:   SplitSink<Framed<TcpStream, PrefixFrameCodec>>,
    receiver: SplitStream<Framed<TcpStream, PrefixFrameCodec>>,

    symmetric_key: SymmetricKey,
}

pub struct ChannelNew {
    state: ChannelNewState,

    // The parts we received
    received_public_key:   Option<PublicKey>,
    received_rand_value:   Option<RandValue>,

    ephemeral_private_key: Option<DhPrivateKey>,

    // The parts used to generate signature.
    key_salt:   Option<Salt>,
    public_key: Option<DhPublicKey>,

    rng: DummyRandom<StdRng>,
    security_module_client: SecurityModuleClient,

    sender:   Option<SplitSink<Framed<TcpStream, PrefixFrameCodec>>>,
    receiver: Option<SplitStream<Framed<TcpStream, PrefixFrameCodec>>>,
}

enum ChannelNewState {
    // Connecting to the given address
    Connecting(TcpStreamNew),

    // Waiting public key from SecurityModuleClient
    WaitingPublicKey(Box<Future<Item=PublicKey, Error=SecurityModuleClientError>>),

    // Trying to send serialized InitChannel message
    SendingInit(Option<Vec<u8>>),

    // Waiting the InitChannel message from neighbor
    WaitingInit,

    // Waiting signature from SecurityModuleClient
    WaitingSignature(Box<Future<Item=Signature, Error=SecurityModuleClientError>>),

    // Trying to send serialized Exchange message
    SendingExchange(Option<Vec<u8>>),

    // Waiting the Exchange message from neighbor
    WaitingExchange,

    // The handshake finished, we need this state for the limitation of lifetime module
    Finished(SymmetricKey),
    Empty,
}

impl Channel {
    /// Create a new channel connected to the specified neighbor.
    pub fn connect(handle: &Handle, addr: &SocketAddr, security_module_client: &SecurityModuleClient) -> ChannelNew {
        ChannelNew {
            state: ChannelNewState::Connecting(TcpStream::connect(addr, handle)),
            security_module_client: security_module_client.clone(),
            rng: DummyRandom::new(&[1, 2, 3, 4, 5, 6]), // FIXME:
            sender:   None,
            receiver: None,
            received_public_key:   None,
            received_rand_value:   None,
            ephemeral_private_key: None,
            key_salt:   None,
            public_key: None,
        }
    }

    // Create a new channel from a incoming socket.
    pub fn from_socket(handle: &Handle, socket: TcpStream, security_module_client: &SecurityModuleClient) -> ChannelNew {
        let (sender, receiver) = socket.framed(PrefixFrameCodec::new()).split();

        let public_key_fut = security_module_client.request_public_key();

        ChannelNew {
            state: ChannelNewState::WaitingPublicKey(Box::new(public_key_fut)),
            security_module_client: security_module_client.clone(),
            rng: DummyRandom::new(&[1, 2, 3, 4, 5, 6]), // FIXME:
            sender:   Some(sender),
            receiver: Some(receiver),
            received_public_key:   None,
            received_rand_value:   None,
            ephemeral_private_key: None,
            key_salt:   None,
            public_key: None,
        }
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
                        let (sender, receiver) = tcp_stream.framed(PrefixFrameCodec::new()).split();
                        self.sender   = Some(sender);
                        self.receiver = Some(receiver);

                        let public_key_fut = self.security_module_client.request_public_key();

                        mem::replace(&mut self.state, ChannelNewState::WaitingPublicKey(Box::new(public_key_fut)));
                    }
                    Async::NotReady => {
                        mem::replace(&mut self.state, ChannelNewState::Connecting(stream_new));
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::WaitingPublicKey(mut boxed_public_key_fut) => {
                match boxed_public_key_fut.poll()? {
                    Async::Ready(public_key) => {
                        let mut message = ::capnp::message::Builder::new_default();
                        // Create InitChannel message
                        {
                            let mut init_channel = message.init_root::<init_channel::Builder>();
                            // Set neighborPublicKey
                            {
                                let mut neighbor_public_key =
                                    init_channel.borrow().init_neighbor_public_key();
                                let public_key_bytes = Bytes::from(public_key.as_bytes());

                                write_custom_u_int256(&mut neighbor_public_key, &public_key_bytes)?;
                            }
                            // Set channelRandValue
                            {
                                let mut channel_rand_value =
                                    init_channel.borrow().init_channel_rand_value();
                                let rand_value = RandValue::new(&self.rng);
                                let rand_value_bytes = Bytes::from(rand_value.as_bytes());

                                write_custom_u_int128(&mut channel_rand_value, &rand_value_bytes)?;
                            }
                        }

                        // TODO: Support async serialize
                        let mut serialized_msg = Vec::new();
                        serialize_packed::write_message(&mut serialized_msg, &message)?;

                        // Transfer state
                        mem::replace(&mut self.state, ChannelNewState::SendingInit(Some(serialized_msg)));
                    }
                    Async::NotReady => {
                        mem::replace(&mut self.state, ChannelNewState::WaitingPublicKey(boxed_public_key_fut));
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::SendingInit(serialize_msg) => {
                match self.sender {
                    None => unreachable!(),
                    Some(ref mut sender) => {
                        if let Some(msg) = serialize_msg {
                            match sender.start_send(msg)? {
                                AsyncSink::Ready => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingInit(None));
                                }
                                AsyncSink::NotReady(b) => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingInit(Some(b)));
                                }
                            }
                        } else {
                            match sender.poll_complete()? {
                                Async::NotReady => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingInit(None));
                                }
                                Async::Ready(_) => {
                                    mem::replace(&mut self.state, ChannelNewState::WaitingInit);
                                }
                            }
                        }
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::WaitingInit => {
                match self.receiver {
                    None => unreachable!(),
                    Some(ref mut receiver) => {
                        if let Async::Ready(Some(buf)) = receiver.poll()? {
                            // Read initChannel message
                            {
                                let mut buffer = io::Cursor::new(buf);
                                let message_rdr = serialize_packed::read_message(&mut buffer,::capnp::message::ReaderOptions::new())?;

                                let init_channel = message_rdr.get_root::<init_channel::Reader>()?;
                                // Read the neighborPublicKey
                                {
                                    let neighbor_public_key = init_channel.get_neighbor_public_key()?;

                                    let mut public_key_bytes = BytesMut::with_capacity(32);
                                    read_custom_u_int256(&mut public_key_bytes, &neighbor_public_key)?;

                                    // FIXME: Remove unwrap usage
                                    self.received_public_key = Some(PublicKey::from_bytes(&public_key_bytes).unwrap());
                                }
                                // Read the channelRandValue
                                {
                                    let rand_value = init_channel.get_channel_rand_value()?;

                                    let mut rand_value_bytes = BytesMut::with_capacity(16);
                                    read_custom_u_int128(&mut rand_value_bytes, &rand_value)?;

                                    // FIXME: Remove unwrap usage
                                    self.received_rand_value = Some(RandValue::from_bytes(&rand_value_bytes).unwrap());
                                }
                            }

                            // Generate ephemeral DH private key
                            let key_salt = Salt::new(&self.rng);
                            let ephemeral_private_key = DhPrivateKey::new(&self.rng);
                            let comm_public_key = ephemeral_private_key.compute_public_key();

                            let rand_value = match self.received_rand_value {
                                None => unreachable!("can not found channelRandValue"),
                                Some(ref rand_value) => rand_value.clone(),
                            };

                            // Calculate the message for signature
                            // message = (channelRandValue + commPublicKey + keySalt)
                            let mut msg_to_sign = Vec::with_capacity(3 * 256 + 1);
                            msg_to_sign.extend_from_slice(rand_value.as_bytes());
                            msg_to_sign.extend_from_slice(comm_public_key.as_bytes());
                            msg_to_sign.extend_from_slice(key_salt.as_bytes());

                            // Request signature from SecurityModuleClient
                            let signature_fut = self.security_module_client.request_sign(msg_to_sign);

                            // Save
                            self.key_salt = Some(key_salt);
                            self.public_key = Some(ephemeral_private_key.compute_public_key());
                            self.ephemeral_private_key = Some(ephemeral_private_key);

                            mem::replace(&mut self.state, ChannelNewState::WaitingSignature(Box::new(signature_fut)));
                        } else {
                            mem::replace(&mut self.state, ChannelNewState::WaitingInit);
                        }
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::WaitingSignature(mut boxed_signature_fut) => {
                match boxed_signature_fut.poll()? {
                    Async::Ready(signature) => {
                        let mut message = ::capnp::message::Builder::new_default();
                        // Create Exchange message
                        {
                            let mut ex = message.init_root::<exchange::Builder>();
                            // Feed the commPublicKey
                            {
                                let mut ex_comm_public_key = ex.borrow().init_comm_public_key();
                                let comm_public_key_bytes = match self.public_key {
                                    None => unreachable!("can not found commPublicKey"),
                                    Some(ref comm_public_key) => Bytes::from(comm_public_key.as_bytes()),
                                };

                                write_custom_u_int256(&mut ex_comm_public_key, &comm_public_key_bytes)?;
                            }
                            // Feed the keySalt
                            {
                                let mut ex_key_salt = ex.borrow().init_key_salt();
                                let key_salt_bytes = match self.key_salt {
                                    None => unreachable!("can not found keySalt"),
                                    Some(ref key_salt) => Bytes::from(key_salt.as_bytes()),
                                };
                                write_custom_u_int256(&mut ex_key_salt, &key_salt_bytes)?;
                            }
                            // Feed the signature
                            {
                                let mut ex_signature = ex.borrow().init_signature();
                                let signature_bytes = Bytes::from(signature.as_bytes());
                                write_custom_u_int256(&mut ex_signature, &signature_bytes)?;
                            }
                        }

                        let mut serialized_msg = Vec::new();
                        serialize_packed::write_message(&mut serialized_msg, &message)?;

                        mem::replace(&mut self.state, ChannelNewState::SendingExchange(Some(serialized_msg)));
                    }
                    Async::NotReady => {
                        mem::replace(&mut self.state, ChannelNewState::WaitingSignature(boxed_signature_fut));
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::SendingExchange(serialize_msg) => {
                match self.sender {
                    None => unreachable!(),
                    Some(ref mut sender) => {
                        if let Some(msg) = serialize_msg {
                            match sender.start_send(msg)? {
                                AsyncSink::Ready => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingExchange(None));
                                }
                                AsyncSink::NotReady(msg) => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingExchange(Some(msg)));
                                }
                            }
                        } else {
                            match sender.poll_complete()? {
                                Async::NotReady => {
                                    mem::replace(&mut self.state, ChannelNewState::SendingExchange(None));
                                }
                                Async::Ready(_) => {
                                    mem::replace(&mut self.state, ChannelNewState::WaitingExchange);
                                }
                            }
                        }
                    }
                }
                Ok(Async::NotReady)
            }
            ChannelNewState::WaitingExchange => {
                match self.receiver {
                    None => unreachable!(),
                    Some(ref mut receiver) => {
                        if let Async::Ready(Some(buf)) = receiver.poll()? {
                            // Read Exchange message
                            let mut public_key_bytes = BytesMut::with_capacity(32);
                            let mut key_salt_bytes   = BytesMut::with_capacity(32);
                            let mut signature_bytes  = BytesMut::with_capacity(32);

                            {
                                let mut buffer = io::Cursor::new(buf);
                                let message_rdr = serialize_packed::read_message(&mut buffer,::capnp::message::ReaderOptions::new())?;

                                let ex = message_rdr.get_root::<exchange::Reader>()?;
                                // Read commPublicKey
                                {
                                    let public_key = ex.get_comm_public_key()?;
                                    read_custom_u_int256(&mut public_key_bytes, &public_key)?;
                                }
                                // Read keySalt
                                {
                                    let key_salt = ex.get_key_salt()?;
                                    read_custom_u_int256(&mut key_salt_bytes, &key_salt)?;
                                }
                                // Read signature
                                {
                                    let signature = ex.get_signature()?;
                                    read_custom_u_int256(&mut signature_bytes, &signature)?;
                                }
                            }

                            let public_key = DhPublicKey::from_bytes(public_key_bytes.as_ref()).unwrap();
                            let key_salt   = Salt::from_bytes(key_salt_bytes.as_ref()).unwrap();
                            let signature  = Signature::from_bytes(signature_bytes.as_ref()).unwrap();

                            let mut message = Vec::new();
                            message.extend_from_slice(public_key.as_bytes());
                            message.extend_from_slice(key_salt.as_bytes());
                            message.extend_from_slice(signature.as_bytes());

                            let received_public = match self.received_public_key {
                                None => unreachable!(),
                                Some(ref key) => key,
                            };

                            if ::crypto::identity::verify_signature(&message, received_public, &signature) {
                                let ephemeral_private_key = mem::replace(&mut self.ephemeral_private_key, None).unwrap();
                                let symmetric_key = ephemeral_private_key.derive_symmetric_key(&public_key, &key_salt);

                                mem::replace(&mut self.state, ChannelNewState::Finished(symmetric_key));

                                Ok(Async::NotReady)
                            } else {
                                Err(ChannelError::InvalidSignature)
                            }
                        } else {
                            mem::replace(&mut self.state, ChannelNewState::WaitingExchange);
                            Ok(Async::NotReady)
                        }
                    }
                }
            }
            ChannelNewState::Finished(key) => {
                Ok(Async::Ready(Channel {
                    symmetric_key: key,
                    sender: mem::replace(&mut self.sender, None).unwrap(),
                    receiver: mem::replace(&mut self.receiver, None).unwrap(),
                }))
            }
            ChannelNewState::Empty => unreachable!("can't poll twice"),
        }
    }
}

#[allow(unused)]
pub fn create_channel(handle: &Handle, socket_addr: SocketAddr, neighbor_public_key: &PublicKey)
                      -> impl Future<Item=(), Error=ChannelError> {
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
    Ok(()).into_future()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_channel_connect() {
        // TODO
    }
}