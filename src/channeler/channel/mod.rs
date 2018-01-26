use std::{io, mem, cell::RefCell, rc::Rc};
use std::net::SocketAddr;

use bytes::Bytes;
use futures::prelude::*;
use futures::stream::{SplitSink, SplitStream};
use futures::sync::mpsc;
use ring::rand::SecureRandom;
use tokio_core::net::TcpStream;
use tokio_core::reactor::{Handle};
use tokio_io::AsyncRead;
use tokio_io::codec::Framed;

use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use crypto::identity::{verify_signature, PublicKey};
use crypto::rand_values::RandValue;
use crypto::sym_encrypt::{Decryptor, EncryptNonceCounter, Encryptor, SymEncryptError};
use security_module::client::{SecurityModuleClient, SecurityModuleClientError};

use proto::{Schema, SchemaError};
use proto::channeler::{
    deserialize_message, serialize_message, EncryptMessage, Exchange, InitChannelActive,
    InitChannelPassive,
};

use timer::messages::FromTimer;
use bytes::{Buf, BigEndian};

use super::{NeighborsTable, messages::*, KEEP_ALIVE_TICKS};

const TIMEOUT_TICKS: usize = 100;

use super::codec::{Codec, CodecError};

type FramedSink = SplitSink<Framed<TcpStream, Codec>>;
type FramedStream = SplitStream<Framed<TcpStream, Codec>>;

#[derive(Debug)]
pub enum ChannelError {
    Io(io::Error),
    Ring(::ring::error::Unspecified),
    Timeout,
    AsyncMutexError,
    Schema(SchemaError),
    Capnp(::capnp::Error),
    Codec(CodecError),
    SecurityModule(SecurityModuleClientError),
    EncryptError(SymEncryptError),
    SendToNetworkerFailed,
    SendToRemoteFailed,
    RecvFromInnerFailed,
    RecvFromTimerFailed,
    KeepAliveTimeout,
    // FIXME: Use enum instead of reason string?
    Closed(&'static str),
}

/// The encrypted channel used to communicate with neighbors.
#[must_use = "futures do nothing unless polled"]
pub struct Channel<R> {
    // Remote identity public key
    remote_public_key: PublicKey,

    // Secure random number generator
    secure_rng: Rc<R>,

    // The inner sender and receiver
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    channeler_receiver: mpsc::Receiver<ToChannel>,
    networker_buffered: Option<ChannelerToNetworker>,

    // The outer sender and receiver
    outer_sender: FramedSink,
    outer_receiver: FramedStream,
    outer_buffered: Option<Bytes>,

    send_counter: u64,
    recv_counter: u64,
    encryptor: Encryptor,
    decryptor: Decryptor,

    send_keepalive_ticks: usize,
    recv_keepalive_ticks: usize,
}

impl<R: SecureRandom + 'static> Channel<R> {
    /// Create a new channel from an incoming socket.
    pub fn from_socket(
        socket: TcpStream,
        // handle: &Handle,
        neighbors: Rc<RefCell<NeighborsTable>>,
        networker_sender: &mpsc::Sender<ChannelerToNetworker>,
        sm_client: &SecurityModuleClient,
        timer_receiver: mpsc::Receiver<FromTimer>,
        secure_rng: Rc<R>,
    ) -> ChannelNew<R> {
        // let neighbors_for_task = neighbors.clone();
        let sm_client_for_task = sm_client.clone();
        let secure_rng_for_task = Rc::clone(&secure_rng);

        let (sink, stream) = socket.framed(Codec::new()).split();

        let init_channel_task = stream
            .into_future()
            .map_err(|(e, _)| ChannelError::Codec(e))
            .and_then(|(frame, stream)| match frame {
                Some(frame) => Ok((frame, stream)),
                None => Err(ChannelError::Closed("connection lost")),
                /* TODO CR: Use enum instead of reason string?
                 *
                 * Is closing the channel really an error?
                 *
                 * Is there a different way to close the Channel gracefully in this case?
                 * I'm not sure about this myself yet.
                 */
            })
            .and_then(move |(frame, stream)| {
                // Expects an InitChannelActive message.
                InitChannelActive::decode(frame)
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(move |init_channel_active| {
                        if validate_neighbor(
                            &init_channel_active.neighbor_public_key,
                            &neighbors.borrow(),
                        ) {
                            Ok((init_channel_active, stream))
                        } else {
                            Err(ChannelError::Closed("validate failed"))
                        }
                    })
            })
            .and_then(move |(init_channel_active, stream)| {
                sm_client_for_task
                    .request_public_key()
                    .map_err(ChannelError::SecurityModule)
                    .and_then(move |public_key| {
                        let channel_rand_value = RandValue::new(&*secure_rng_for_task);
                        let init_channel_passive = InitChannelPassive {
                            neighbor_public_key: public_key,
                            channel_rand_value,
                        };
                        Ok((init_channel_active, init_channel_passive, stream))
                    })
            })
            .and_then(|(init_channel_active, init_channel_passive, stream)| {
                init_channel_passive
                    .encode()
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(|init_channel_passive_bytes| {
                        sink.send(init_channel_passive_bytes).map_err(ChannelError::Codec)
                    })
                    .and_then(move |sink| {
                        Ok((init_channel_active, init_channel_passive, sink, stream))
                    })
            });

        ChannelNew {
            state: ChannelNewState::InitChannel(Box::new(init_channel_task)),
            timeout_ticks: TIMEOUT_TICKS,
            timer_receiver: timer_receiver,
            secure_rng,
            sm_client: sm_client.clone(),
            remote_public_key: None,
            networker_sender: networker_sender.clone(),
            channel_index: None,
        }
    }

    /// Create a new channel connected to the specified neighbor.
    pub fn connect(
        addr: &SocketAddr,
        handle: &Handle,
        remote_public_key: &PublicKey,
        // neighbors: Rc<RefCell<NeighborsTable>>,
        networker_sender: &mpsc::Sender<ChannelerToNetworker>,
        sm_client: &SecurityModuleClient,
        timer_receiver: mpsc::Receiver<FromTimer>,
        secure_rng: Rc<R>,
    ) -> ChannelNew<R> {
        // TODO: Add debug assert here to check the neighbor public key and the channel index
        // let _neighbors_for_task = neighbors.clone();
        let sm_client_for_task = sm_client.clone();
        let secure_rng_for_task = Rc::clone(&secure_rng);

        let expected_public_key = remote_public_key.clone();

        let init_channel_task = TcpStream::connect(addr, handle)
            .map_err(|e| e.into())
            .and_then(|socket| Ok(socket.framed(Codec::new()).split()))
            .and_then(move |(sink, stream)| {
                sm_client_for_task
                    .request_public_key()
                    .map_err(ChannelError::SecurityModule)
                    .and_then(move |public_key| {
                        let channel_rand_value = RandValue::new(&*secure_rng_for_task);
                        let init_channel_active = InitChannelActive {
                            neighbor_public_key: public_key,
                            channel_rand_value,
                            channel_index: 0, // FIXME: Remove this field
                        };
                        Ok((init_channel_active, sink, stream))
                    })
            })
            .and_then(|(init_channel_active, sink, stream)| {
                init_channel_active
                    .encode()
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(|init_active_bytes| {
                        sink.send(init_active_bytes).map_err(ChannelError::Codec)
                    })
                    .and_then(move |sink| Ok((init_channel_active, sink, stream)))
            })
            .and_then(|(init_channel_active, sink, stream)| {
                stream
                    .into_future()
                    .map_err(|(e, _rx)| ChannelError::Codec(e))
                    .and_then(|(frame, stream)| match frame {
                        Some(frame) => Ok((init_channel_active, frame, sink, stream)),
                        None => Err(ChannelError::Closed("connection lost")),
                    })
            })
            .and_then(move |(init_channel_active, frame, sink, stream)| {
                InitChannelPassive::decode(frame)
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(move |init_channel_passive| {
                        if init_channel_passive.neighbor_public_key != expected_public_key {
                            Err(ChannelError::Closed("unexpected identity"))
                        } else {
                            Ok((init_channel_active, init_channel_passive, sink, stream))
                        }
                    })
            });

        ChannelNew {
            state: ChannelNewState::InitChannel(Box::new(init_channel_task)),
            timeout_ticks: TIMEOUT_TICKS,
            timer_receiver,
            secure_rng,
            sm_client: sm_client.clone(),
            remote_public_key: None,
            networker_sender: networker_sender.clone(),
            channel_index: Some(0), // FIXME: Remove this field
        }
    }

    pub fn remote_public_key(&self) -> PublicKey {
        self.remote_public_key.clone()
    }

    /// Pack and encrypt a message to be sent to remote.
    fn pack_msg(
        &mut self,
        message_type: MessageType,
        content: Bytes,
    ) -> Result<Bytes, ChannelError> {
        let (padding_len, padding_bytes) = gen_padding(&*self.secure_rng)?;

        let encrypt_message = EncryptMessage {
            inc_counter: self.send_counter,
            // TODO CR: I realize now that we are using two increasing counters simultaneously:
            // The one in the encryption nonce (EncryptNonceCounter inside
            // sym_encrypt.rs) and inc_counter here. Maybe using EncryptNonceCounter
            // should be enough. If we want to use EncryptNonceCounter we should
            // change the interface of sym_encrypt to allow giving the counter as nonce
            // argument. I am still not sure if doing this is insecure in some way. We need to
            // discuss this.
            rand_padding: Bytes::from(&padding_bytes[..padding_len as usize]),
            message_type,
            content,
        };

        let encrypted = self.encryptor.encrypt(&encrypt_message.encode()?)?;

        serialize_message(Bytes::from(encrypted))
            .map_err(|e| e.into())
            .and_then(|bytes| {
                // TODO CR: I think that it is surprising that pack_msg increases the
                // send_counter. Maybe we should give the current
                // self.send_counter as argument to pack_msg, and incremenet it
                // outside of pack_msg? See also increasing of counter inside
                // unpack_msg function. What do you think?
                increase_counter(&mut self.send_counter);
                Ok(bytes)
            })
    }

    /// Decrypt and unpack a message received from remote.
    fn unpack_msg(&mut self, raw: Bytes) -> Result<EncryptMessage, ChannelError> {
        let plain_msg = Bytes::from(self.decryptor.decrypt(&deserialize_message(raw)?)?);

        EncryptMessage::decode(plain_msg)
            .map_err(|e| e.into())
            .and_then(|msg| {
                if msg.inc_counter != self.recv_counter {
                    Err(ChannelError::Closed("unexpected counter"))
                    // TODO CR: I think that we should have an Enum for the closing reason instead
                    // of using strings.
                } else {
                    increase_counter(&mut self.recv_counter);
                    Ok(msg)
                }
            })
    }

    /// Attempt to send a message to Networker. If the channel is not ready, we buffer the
    /// message and it will be sent later. This function may only be called if no other messages
    /// are planned for sending to the Networker.
    fn try_start_send_networker(&mut self, msg: ChannelerToNetworker) -> Poll<(), ChannelError> {
        debug_assert!(self.networker_buffered.is_none());

        self.networker_sender
            .start_send(msg)
            .map_err(|_| ChannelError::SendToNetworkerFailed)
            .and_then(|start_send| {
                if let AsyncSink::NotReady(msg) = start_send {
                    self.networker_buffered = Some(msg);
                    Ok(Async::NotReady)
                } else {
                    Ok(Async::Ready(()))
                }
            })
    }

    /// Attempt to send a message through the socket. If the channel is not ready, we buffer the
    /// message and it will be sent later. This function may only be called if no other messages
    /// are planned for sending to the socket.
    fn try_start_send_outer(&mut self, msg: Bytes) -> Poll<(), ChannelError> {
        debug_assert!(self.outer_buffered.is_none());

        self.outer_sender
            .start_send(msg)
            .map_err(|e| e.into())
            .and_then(|start_send| {
                if let AsyncSink::NotReady(msg) = start_send {
                    self.outer_buffered = Some(msg);
                    Ok(Async::NotReady)
                } else {
                    Ok(Async::Ready(()))
                }
            })
    }

    /// Attempt to pull out the next message **needed to be sent to remote**.
    fn try_poll_inner(&mut self) -> Poll<Option<Bytes>, ChannelError> {
        loop {
            let poll_result = self.channeler_receiver
                .poll()
                .map_err(|_| ChannelError::RecvFromInnerFailed);

            if let Some(msg) = try_ready!(poll_result) {
                match msg {
                    ToChannel::TimeTick => {
                        // TODO CR:
                        // Is there any danger here for panic! ?
                        // Is it possible that at this point send_ka_ticks == 0?
                        self.send_keepalive_ticks -= 1;
                        self.recv_keepalive_ticks -= 1;

                        if self.recv_keepalive_ticks == 0 {
                            return Err(ChannelError::KeepAliveTimeout);
                        } else if self.send_keepalive_ticks == 0 {
                            self.send_keepalive_ticks = KEEP_ALIVE_TICKS;

                            let msg = self.pack_msg(MessageType::KeepAlive, Bytes::new())?;
                            return Ok(Async::Ready(Some(msg)));
                        }
                    }
                    ToChannel::SendMessage(raw) => {
                        let msg = self.pack_msg(MessageType::User, raw)?;
                        return Ok(Async::Ready(Some(msg)));
                    }
                }
            } else {
                // The internal channel has finish
                return Ok(Async::Ready(None));
            }
        }
    }

    /// Attempt to pull out the next message **needed to be sent to networker**.
    fn try_poll_outer(&mut self) -> Poll<Option<ChannelerToNetworker>, ChannelError> {
        loop {
            if let Some(raw) = try_ready!(self.outer_receiver.poll()) {
                let msg = self.unpack_msg(raw)?;

                match msg.message_type {
                    MessageType::KeepAlive => {
                        trace!("recv keepalive from {:?}", self.remote_public_key);
                        self.recv_keepalive_ticks = 2 * KEEP_ALIVE_TICKS;
                    }
                    MessageType::User => {
                        let msg = ChannelerToNetworker {
                            remote_public_key: self.remote_public_key.clone(),
                            event: ChannelEvent::Message(msg.content),
                        };

                        return Ok(Async::Ready(Some(msg)));
                    }
                }
            } else {
                // The tcp connection has finished
                return Ok(Async::Ready(None));
            }
        }
    }
}

// TODO CR: We might be able to split the file here, as this file is pretty
// long. I'm still not sure about this, as it might be more comfortable to have
// everything together here. What do you think?

// TODO CR:
// I think that we might be able to change this code (impl Future for Channel)
// to use combinators. This might save us all the hassle of the buffered items,
// and could probably produce shorter code.
//
// What we basically do here is try to get an item from try_poll_inner and push
// it into send_outer. At the same time we try to get an item from poll_outer
// and push it into send_inner. It's like connecting two sinks and streams
// together.
//
// I think that doing this kind of refactoring here will be worth it, because
// the code could become much easier to reason about.
//
impl<R: SecureRandom + 'static> Future for Channel<R> {
    type Item = ();
    type Error = ChannelError;

    fn poll(&mut self) -> Poll<(), Self::Error> {
        loop {
            if let Some(msg) = self.outer_buffered.take() {
                try_ready!(self.try_start_send_outer(msg));
            }
            match self.try_poll_inner()? {
                Async::Ready(None) => {
                    debug!("inner channel closed, closing");
                    // TODO CR: Will this code close everything in the channel?
                    // Will the socket be properly closed in this case? Maybe this happens
                    // automatically in rust?
                    // FIXME: Call close() and wait TCP stream closed
                    return Ok(Async::Ready(()));
                }
                Async::Ready(Some(msg)) => {
                    try_ready!(self.try_start_send_outer(msg));
                }
                Async::NotReady => {
                    try_ready!(self.outer_sender.poll_complete());

                    // TODO CR: I think that we might be able to take this code outside of the
                    // scope (Right after the block of match self.try_poll_inner()?), making the
                    // code more symmetric with respect to inner_buffered and outer_buffered.
                    if let Some(msg) = self.networker_buffered.take() {
                        try_ready!(self.try_start_send_networker(msg));
                    }
                    match self.try_poll_outer()? {
                        Async::Ready(None) => {
                            debug!("tcp connection closed, closing");
                            return Ok(Async::Ready(()));
                        }
                        Async::Ready(Some(msg)) => {
                            try_ready!(self.try_start_send_networker(msg));
                        }
                        Async::NotReady => {
                            try_ready!(
                                self.networker_sender
                                    .poll_complete()
                                    .map_err(|_| ChannelError::SendToNetworkerFailed)
                            );
                            return Ok(Async::NotReady);
                        }
                    }
                }
            }
        }
    }
}

enum ChannelNewState {
    /// The initial stage to establish an encrypted channel.
    ///
    /// For the active end, there are several things need to be done:
    ///
    /// 1. Establish the TCP connection
    /// 2. Send `InitChannelActive` message to remote
    /// 3. Wait for `InitChannelPassive` message from remote
    /// 4. Verify the identity contained in received message
    ///
    /// For the passive end, there are several things need to be done:
    ///
    /// 1. Wait for `InitChannelActive` message from remote
    /// 2. Verify the identity contained in received message
    /// 3. Send `InitChannelPassive` message to remote
    ///
    /// If this stage completed successfully, the `Future` resolved and return
    /// the `InitChannelActive`, `InitChannelPassive` message, also return the
    /// underlying `FramedSink` and `FramedStream` which needed in the future
    /// for communicating between two parties.
    InitChannel(
        Box<
            Future<
                Item=(InitChannelActive, InitChannelPassive, FramedSink, FramedStream),
                Error=ChannelError,
            >,
        >,
    ),

    /// The key exchange stage to establish an encrypted channel.
    ///
    /// At this stage, two parties need to do the following steps:
    ///
    /// 1. Generate a ephemeral private key
    /// 2. Send the `Exchange` message to the remote
    /// 3. Wait for `Exchange` message from remote
    /// 4. Verify the `signature` contained in received message
    ///
    /// If this stage completed successfully, the `Future` resolved and return
    /// the `Exchange` message we sent, followed by the `Exchange` message we
    /// received, also return the underlying `FramedSink` and `FramedStream`
    /// which needed to build a `Channel`.
    Exchange(
        Box<
            Future<
                Item=(Exchange, Exchange, DhPrivateKey, FramedSink, FramedStream),
                Error=ChannelError,
            >,
        >,
    ),

    Empty,
}

#[must_use = "futures do nothing unless polled"]
pub struct ChannelNew<R> {
    state: ChannelNewState,
    timeout_ticks: usize,
    timer_receiver: mpsc::Receiver<FromTimer>,

    secure_rng: Rc<R>,
    sm_client: SecurityModuleClient,
    channel_index: Option<u32>,
    networker_sender: mpsc::Sender<ChannelerToNetworker>,
    remote_public_key: Option<PublicKey>,
}

impl<R: SecureRandom> ChannelNew<R> {
    fn create_exchange_task(
        remote_public_key: PublicKey,
        sent_rand_value: RandValue,
        recv_rand_value: RandValue,
        sink: FramedSink,
        stream: FramedStream,
        sm_client: &SecurityModuleClient,
        secure_rng: Rc<R>,
    ) -> impl Future<
        Item=(Exchange, Exchange, DhPrivateKey, FramedSink, FramedStream),
        Error=ChannelError,
    > {
        // Generate ephemeral DH private key
        let key_salt = Salt::new(&*secure_rng);
        let comm_private_key = DhPrivateKey::new(&*secure_rng);
        let comm_public_key = comm_private_key.compute_public_key();

        // message = (channelRandValue + commPublicKey + keySalt)
        let msg = create_message_to_sign(&recv_rand_value, &comm_public_key, &key_salt);

        sm_client
            .request_sign(msg)
            .map_err(ChannelError::SecurityModule)
            .and_then(move |signature| {
                Ok(Exchange {
                    comm_public_key,
                    key_salt,
                    signature,
                })
            })
            .and_then(move |exchange| {
                exchange
                    .encode()
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(|exchange_bytes| {
                        sink.send(exchange_bytes).map_err(ChannelError::Codec)
                    })
                    .and_then(move |sink| {
                        stream
                            .into_future()
                            .map_err(|(e, _rx)| ChannelError::Codec(e))
                            .and_then(move |(frame, stream)| match frame {
                                Some(frame) => Ok((exchange, frame, sink, stream)),
                                None => Err(ChannelError::Closed("connection lost")),
                            })
                    })
            })
            .and_then(move |(sent_exchange, frame, sink, stream)| {
                Exchange::decode(frame)
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(move |recv_exchange| {
                        let msg = create_message_to_sign(
                            &sent_rand_value,
                            &recv_exchange.comm_public_key,
                            &recv_exchange.key_salt,
                        );

                        if verify_signature(&msg, &remote_public_key, &recv_exchange.signature) {
                            Ok((sent_exchange, recv_exchange, comm_private_key, sink, stream))
                        } else {
                            Err(ChannelError::Closed("invalid signature"))
                        }
                    })
            })
    }
}

// TODO CR:
// I think that the order of events here is very
// serial: First InitChannel, we drive it to completion and then we do Exchange.
// In my opinion we can make the code much shorter here if we use something like
// InitChannel... .and_then(... Exchange ... )
// If I don't miss anything here, I think that this is what we currently do inside the ChannelNew
// state machine.
//
// The current state machine design does give us the advantage of having a proper type for this
// future (ChannelNew), but we can always Box the result of combinators or use some impl
// Future<...>. We already Box the contents of ChannelNewState, so it's not much of an efficiency
// loss here.
//
// Answer: Hard to debug when using the combinator.
impl<R: SecureRandom + 'static> Future for ChannelNew<R> {
    type Item = (mpsc::Sender<ToChannel>, Channel<R>);
    type Error = ChannelError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("ChannelNew::poll - {:?}", ::std::time::Instant::now());

        loop {
            let poll_timer = self.timer_receiver.poll()
                .map_err(|_| ChannelError::RecvFromTimerFailed);

            if poll_timer?.is_ready() {
                if self.timeout_ticks > 0 {
                    self.timeout_ticks -= 1;
                } else {
                    return Err(ChannelError::Timeout);
                }
            }

            match mem::replace(&mut self.state, ChannelNewState::Empty) {
                ChannelNewState::Empty => unreachable!("invalid state"),
                ChannelNewState::InitChannel(mut init_channel_task) => {
                    match init_channel_task.poll()? {
                        Async::Ready((init_channel_active, init_channel_passive, sink, stream)) => {
                            trace!("ChannelNewState::InitialChannel [Ready]");

                            let sent_rand_value: RandValue;
                            let recv_public_key: PublicKey;
                            let recv_rand_value: RandValue;

                            if self.channel_index.is_some() {
                                sent_rand_value = init_channel_active.channel_rand_value.clone();
                                recv_public_key = init_channel_passive.neighbor_public_key.clone();
                                recv_rand_value = init_channel_passive.channel_rand_value.clone();
                            } else {
                                sent_rand_value = init_channel_passive.channel_rand_value.clone();
                                recv_public_key = init_channel_active.neighbor_public_key.clone();
                                recv_rand_value = init_channel_active.channel_rand_value.clone();

                                self.channel_index = Some(init_channel_active.channel_index);
                            }

                            self.remote_public_key = Some(recv_public_key.clone());

                            let exchange_task = ChannelNew::create_exchange_task(
                                recv_public_key,
                                sent_rand_value,
                                recv_rand_value,
                                sink,
                                stream,
                                &self.sm_client,
                                Rc::clone(&self.secure_rng),
                            );

                            self.state = ChannelNewState::Exchange(Box::new(exchange_task));
                        }
                        Async::NotReady => {
                            trace!("ChannelNewState::InitialChannel [Not Ready]");
                            self.state = ChannelNewState::InitChannel(init_channel_task);
                            return Ok(Async::NotReady);
                        }
                    }
                }
                ChannelNewState::Exchange(mut exchange_task) => {
                    // TODO CR: Could we somehow write this code so that we don't need to have
                    // those asserts for existence of channel_index and remote_public_key?
                    // Also note the channel_index.take().expect(...) below.
                    //
                    // Maybe those values should be inside exchange_task, or we should pass them in
                    // some other way? I think that we are giving up some safety from the compiler
                    // here by using Option<> for these types.
                    //
                    // Answer:
                    //
                    // What the problem here?
                    // I think these assertions can prevent us from making mistakes, if you have
                    // any idea, you can change the logic here.
                    debug_assert!(self.channel_index.is_some());
                    debug_assert!(self.remote_public_key.is_some());

                    match exchange_task.poll()? {
                        Async::Ready((send_exchange, recv_exchange, my_private_key, sink, stream)) => {
                            trace!("ChannelNewState::Exchange       [Ready]");
                            // Combine self private key, received public key
                            // and sent key salt to derive symmetric key for
                            // sending data
                            let key_send = my_private_key.derive_symmetric_key(
                                &recv_exchange.comm_public_key,
                                &send_exchange.key_salt,
                            );

                            // Combine self private key, received public key
                            // and received key salt to derive symmetric key
                            // for receiving data
                            let key_recv = my_private_key.derive_symmetric_key(
                                &recv_exchange.comm_public_key,
                                &recv_exchange.key_salt,
                            );

                            let encrypt_nonce_counter = EncryptNonceCounter::new(&*self.secure_rng);

                            let encryptor = Encryptor::new(&key_send, encrypt_nonce_counter);
                            let decryptor = Decryptor::new(&key_recv);

                            let (inner_tx, inner_rx) = mpsc::channel::<ToChannel>(0);

                            // TODO CR: See comment beloew about the debug_assert!-s.
                            // We might be able to design this part in a different way, to avoid
                            // the many expect()-s and assert!s. In the meanwhile, we can make sure
                            // that we do this only once, maybe in the beginning of the function.
                            // let channel_index = self.channel_index
                            //     .take()
                            //     .expect("missing channel index");

                            let remote_public_key = self.remote_public_key
                                .take()
                                .expect("missing remote public key");

                            let channel = Channel {
                                secure_rng: Rc::clone(&self.secure_rng),
                                remote_public_key,
                                networker_sender: self.networker_sender.clone(),
                                channeler_receiver: inner_rx,
                                networker_buffered: None,
                                outer_sender: sink,
                                outer_receiver: stream,
                                outer_buffered: None,
                                send_counter: 0,
                                recv_counter: 0,
                                encryptor,
                                decryptor,
                                send_keepalive_ticks: KEEP_ALIVE_TICKS,
                                recv_keepalive_ticks: 2 * KEEP_ALIVE_TICKS,
                            };

                            return Ok(Async::Ready((inner_tx, channel)));
                        }
                        Async::NotReady => {
                            trace!("ChannelNewState::Exchange       [Not Ready]");
                            self.state = ChannelNewState::Exchange(exchange_task);
                            return Ok(Async::NotReady);
                        }
                    }
                }
            }
        }
    }
}

fn create_message_to_sign(rand_value: &RandValue, comm_public_key: &DhPublicKey, key_salt: &Salt) -> Vec<u8> {
    // message = (channelRandValue + commPublicKey + keySalt)
    let mut msg = Vec::new();

    msg.extend_from_slice(rand_value.as_bytes());
    msg.extend_from_slice(comm_public_key.as_bytes());
    msg.extend_from_slice(key_salt.as_bytes());

    msg
}

/// Cyclic increase of counter.
#[inline]
fn increase_counter(counter: &mut u64) {
    if *counter == u64::max_value() {
        *counter = 0;
    } else {
        *counter += 1;
    }
}

fn gen_padding(secure_rng: &SecureRandom)
    -> Result<(usize, [u8; MAX_PADDING_LEN]), ChannelError> {
    debug_assert!((u16::max_value() as usize + 1) % MAX_PADDING_LEN == 0);

    const UINT16_LEN: usize = 2;

    let mut bytes = [0x00; UINT16_LEN];

    secure_rng.fill(&mut bytes[..])?;

    let mut rdr = io::Cursor::new(&bytes[..]);

    let padding_len = (rdr.get_u16::<BigEndian>() as usize) % MAX_PADDING_LEN;

    let mut padding_bytes = [0x00; MAX_PADDING_LEN];

    secure_rng.fill(&mut padding_bytes[..padding_len])?;

    Ok((padding_len, padding_bytes))
}

/// Validate whether `remote_public_key` may actively initiate a channel
fn validate_neighbor(remote_public_key: &PublicKey, neighbors: &NeighborsTable) -> bool {
    match neighbors.get(remote_public_key) {
        None => false,
        Some(neighbor) => {
            neighbor.info.socket_addr.is_none() && neighbor.channel.is_none()
        }
    }
}

// ===== Error Conversion =====

impl From<io::Error> for ChannelError {
    fn from(e: io::Error) -> ChannelError {
        ChannelError::Io(e)
    }
}

impl From<SchemaError> for ChannelError {
    fn from(e: SchemaError) -> ChannelError {
        ChannelError::Schema(e)
    }
}

impl From<CodecError> for ChannelError {
    fn from(e: CodecError) -> ChannelError {
        ChannelError::Codec(e)
    }
}

impl From<SymEncryptError> for ChannelError {
    fn from(e: SymEncryptError) -> ChannelError {
        ChannelError::EncryptError(e)
    }
}

impl From<SecurityModuleClientError> for ChannelError {
    fn from(e: SecurityModuleClientError) -> ChannelError {
        ChannelError::SecurityModule(e)
    }
}

impl From<::ring::error::Unspecified> for ChannelError {
    fn from(e: ::ring::error::Unspecified) -> ChannelError {
        ChannelError::Ring(e)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;
    use channeler::types::{ChannelerNeighbor, ChannelerNeighborInfo};

    fn gen_channeler_neighbor(byte: u8) -> ChannelerNeighbor {
        let key_bytes = [byte; 32];
        let public_key = PublicKey::from_bytes(&key_bytes).unwrap();

        let info = ChannelerNeighborInfo {
            public_key,
            socket_addr: None,
        };

        ChannelerNeighbor {
            info,
            channel: None,
            retry_ticks: 0,
            num_pending: 0,
        }
    }

    /// This function generate a neighbor table for test usage.
    ///
    /// All of the account generated by the `FixedByteRandom`,
    /// and the they have the following relationship:
    ///
    /// - `0 <-- 1`
    /// - `0 <-- 2`
    /// - `0 --> 3`
    fn gen_neighbors() -> (Vec<PublicKey>, HashMap<PublicKey, ChannelerNeighbor>) {
        let neighbor1 = gen_channeler_neighbor(1);
        let neighbor2 = gen_channeler_neighbor(2);

        let mut neighbor3 = gen_channeler_neighbor(3);
        neighbor3.info.socket_addr = Some("127.0.0.1:1234".parse().unwrap());

        // My public key
        let key_bytes = [0u8; 32];
        let my_public_key = PublicKey::from_bytes(&key_bytes).unwrap();

        let mut keys = Vec::new();
        keys.push(my_public_key);
        keys.push(neighbor1.info.public_key.clone());
        keys.push(neighbor2.info.public_key.clone());
        keys.push(neighbor3.info.public_key.clone());

        let mut neighbors = HashMap::new();
        neighbors.insert(keys[1].clone(), neighbor1);
        neighbors.insert(keys[2].clone(), neighbor2);
        neighbors.insert(keys[3].clone(), neighbor3);

        (keys, neighbors)
    }

    /// According the design specs, when the Channeler receives a `InitChannelActive` message,
    /// it should do the following:
    ///
    /// - Check if the received public key belongs to a neighbor that has the initiator role.
    /// If not, close the connection.
    ///
    /// - Check if the `channelIndex` smaller than the maximum allowed channels. Also check if the
    /// `channelIndex` is not already in used by another TCP connection. If the `channelIndex` is
    /// invalid, close the connection.
    ///
    /// Note: Only the responder would receive the `InitChannelActive` message, because for the
    /// initiator, after it sending a `InitChannelActive` message to remote, it expect to receive
    /// a `InitChannelPassive` message from remote, once he received a message which not conform the
    /// `InitChannelPassive` message's format, the connection would be closed with the `SchemaError`
    ///
    /// Notation: (Initiator, Connected)
    #[test]
    fn test_validation() {
        let (keys, mut neighbors) = gen_neighbors();

        let (tx, _) = mpsc::channel::<ToChannel>(0);
        {
            let neighbor = neighbors.get_mut(&keys[2]).unwrap();
            neighbor.channel = Some(tx);
        }

        // Case 1: (Y, N)
        //
        // - The neighbor has the initiator role.
        // - There is no connected channel between remote.
        let remote_public_key = keys[1].clone();

        assert!(validate_neighbor(&remote_public_key, &neighbors));

        // Case 2: (Y, Y)
        //
        // - The neighbor has the initiator role.
        // - There was a connected channel between remote.
        let remote_public_key = keys[2].clone();

        assert!(!validate_neighbor(&remote_public_key, &neighbors));

        // Case 3: (N, X)
        //
        // - The neighbor doesn't have the initiator role.
        // - There is no connected channel between remote.
        let remote_public_key = keys[3].clone();

        assert!(!validate_neighbor(&remote_public_key, &neighbors));
    }
}
