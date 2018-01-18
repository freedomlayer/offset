use std::{io, mem, time};
use std::net::SocketAddr;
use std::collections::HashMap;

use bytes::Bytes;
use rand::{OsRng, Rng};
use futures::prelude::*;
use futures::sync::mpsc;
use tokio_io::AsyncRead;
use tokio_io::codec::Framed;
use ring::rand::SystemRandom;
use tokio_core::net::TcpStream;
use tokio_core::reactor::{Handle, Timeout};
use futures::stream::{SplitSink, SplitStream};

use utils::{AsyncMutex, AsyncMutexError};
use crypto::rand_values::RandValue;
use crypto::dh::{DhPrivateKey, Salt};
use crypto::identity::{verify_signature, PublicKey};
use crypto::sym_encrypt::{Decryptor, EncryptNonceCounter, Encryptor, SymEncryptError};
use security_module::client::{SecurityModuleClient, SecurityModuleClientError};

use proto::{Schema, SchemaError};
use proto::channeler::{deserialize_message, serialize_message, EncryptMessage, Exchange,
                       InitChannelActive, InitChannelPassive};

use super::{messages::*, types::*, KEEP_ALIVE_TICKS};

use super::codec::{Codec, CodecError};

type FramedStream = SplitStream<Framed<TcpStream, Codec>>;
type FramedSink = SplitSink<Framed<TcpStream, Codec>>;

#[derive(Debug)]
pub enum ChannelError {
    Io(io::Error),
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
    KeepAliveTimeout,
    Closed(&'static str),           // TODO CR: I suggest that we use an Enum instead of strings 
                                    // to indicate the different types of errors here. 
}

/// The encrypted channel used to communicate with neighbors.
#[must_use = "futures do nothing unless polled"] // TODO CR: Why do we have this?
                                                 // I thought futures have this feature
                                                 // automatically, I might be wrong about this
                                                 // though.
pub struct Channel {
    // Remote identity public key
    remote_public_key: PublicKey,

    // The index of this channel
    channel_index: u32,

    os_rng: OsRng,                  // TODO CR: We should take R: SecureRandom instead of the specific OsRng.
                                    // See for example the implementation of RandValue::new().
                                    // Using the more generic SecureRandom will allow us to later
                                    // test this module. On the other hand, if we have OsRng, it
                                    // can be very difficult to have deterministic tests.

    // The inner sender and receiver
    // TODO CR: It's hard for me to understand from the names inner_sender and outer_sender
    // what is the job of each of inner_sender and outer_sender. What other meaningful name
    // could we pick for those?  Maybe networker_sender instead of inner_sender?
    inner_sender: mpsc::Sender<ChannelerToNetworker>,
    inner_receiver: mpsc::Receiver<ToChannel>,
    inner_buffered: Option<ChannelerToNetworker>,

    // The outer sender and receiver
    outer_sender: FramedSink,
    outer_receiver: FramedStream,
    outer_buffered: Option<Bytes>,

    send_counter: u64,
    recv_counter: u64,
    encryptor: Encryptor,
    decryptor: Decryptor,

    send_ka_ticks: usize,           // TODO CR: 
                                    // Maybe we can rename this to send_keepalive_ticks or
                                    // something similar? It took me a while to go through the
                                    // codebase and find out what ka means.
    recv_ka_ticks: usize,
}

impl Channel {
    /// Create a new channel from an incoming socket.
    pub fn from_socket(
        socket: TcpStream,
        neighbors: &AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,      
                    // TODO CR: 
                    // Can we possibly use something weaker than AsyncMutex for neighbors, for example,
                    // RefCell? This is assuming neighbors does not contain any Futures related
                    // items inside.
        networker_tx: &mpsc::Sender<ChannelerToNetworker>,
        // TODO CR: I think that we should change SecurityModuleClient to be a trait instead of a
        // structure. This will make it easier for us to write tests, because we will be able to
        // easily replace SecurityModuleClient with a mock SecurityModuleClient. This should be
        // changed in client.rs of security_module.
        sm_client: &SecurityModuleClient,
        handle: &Handle,
    ) -> ChannelNew {
        // TODO CR: An argument of type R: SecureRandom should be given as argument to this function.
        // Having SystemRandom::new() being called here will cause tests to be non deterministic.
        // We should have just one SystemRandom::new() call for the whole application, and hand
        // references to this instance to all the components.
        //
        // See also the first comment block on:
        // https://github.com/briansmith/ring/blob/master/src/rand.rs
        //      "An application should create a single `SystemRandom` and then use it for
        //      all randomness generation. Functions that generate random bytes should take
        //      a `&SecureRandom` parameter instead of instantiating their own"
        let rng = SystemRandom::new();

        let neighbors_for_task = neighbors.clone();
        let sm_client_for_task = sm_client.clone();
        // Precompute here because the `SystemRandom` does not implement
        // `Clone` trait but we need this value inside the `Future`
        // to create a new `InitChannelPassive` message
        //
        // TODO CR: If it happens to be important to clone SecureRandom, we can make a
        // wrapper type with some kind of Rc (Ref count) to wrap it and clone it around.
        let rand_value = RandValue::new(&rng);

        let (sink, stream) = socket.framed(Codec::new()).split();

        let init_channel_task = stream
            .into_future()
            .map_err(|(e, _)| ChannelError::Codec(e))
            .and_then(|(frame, stream)| match frame {
                Some(frame) => Ok((frame, stream)),
                None => Err(ChannelError::Closed("connection lost")),
                    // TODO CR: I suggest that we have an enum instead of a string
                    // inside ChannelError::Closed.
                    //
                    // Is closing the channel really an error? Is there a different way to close
                    // the Channel gracefully in this case? I'm not sure about this myself yet.
            })
            .and_then(move |(frame, stream)| {
                // Expects an InitChannelActive message.
                InitChannelActive::decode(frame)
                    .into_future()                  // TODO CR: Why do we have into_future() here? Interesting.
                    .map_err(ChannelError::Schema)
                    .and_then(|init_channel_active| {
                        ChannelNew::create_validation_task(
                            init_channel_active.neighbor_public_key.clone(),
                            init_channel_active.channel_index,
                            neighbors_for_task,
                        ).and_then(move |_| Ok((init_channel_active, stream)))
                    })
            })
            .and_then(move |(init_channel_active, stream)| {
                sm_client_for_task
                    .request_public_key()
                    .map_err(ChannelError::SecurityModule)
                    .and_then(move |public_key| {
                        // TODO CR: rand_value should be generated only at this point in time.
                        // Note that currently we precompute it earlier.
                        let init_channel_passive = InitChannelPassive {
                            neighbor_public_key: public_key,
                            channel_rand_value: rand_value,
                        };
                        Ok((init_channel_active, init_channel_passive, stream))
                    })
            })
            .and_then(|(init_channel_active, init_channel_passive, stream)| {
                init_channel_passive
                    .encode()
                    .into_future()
                    .map_err(ChannelError::Schema)
                    .and_then(|init_passive_bytes| {
                        sink.send(init_passive_bytes).map_err(ChannelError::Codec)
                    })
                    .and_then(move |sink| {
                        Ok((init_channel_active, init_channel_passive, sink, stream))
                    })
            });

        ChannelNew {
            state: ChannelNewState::InitChannel(Box::new(init_channel_task)),
            // TODO CR: See similar comment below about using time ticks as the source of time, instead of using
            // time::Duration::from_secs.
            timeout: Timeout::new(time::Duration::from_secs(5), handle).unwrap(),
            rng,
            sm_client: sm_client.clone(),
            remote_public_key: None,
            nw_sender: networker_tx.clone(),
            channel_index: None,
        }
    }

    /// Create a new channel connected to the specified neighbor.
    pub fn connect(
        addr: &SocketAddr,
        neighbor_public_key: &PublicKey,
        channel_index: u32,
        neighbors: &AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
        networker_tx: &mpsc::Sender<ChannelerToNetworker>,
        sm_client: &SecurityModuleClient,
        handle: &Handle,
    ) -> ChannelNew {
        let rng = SystemRandom::new();

        // TODO: Add debug assert here to check the neighbor public kety and the channel index
        let _neighbors_for_task = neighbors.clone();
        let sm_client_for_task = sm_client.clone();
        // Precompute here because the `SystemRandom` not implement
        // `Clone` trait but we need this value inside the `Future`
        // to create a new `InitChannelActive` message
        let rand_value = RandValue::new(&rng);

        let expected_public_key = neighbor_public_key.clone();

        let init_channel_task = TcpStream::connect(addr, handle)
            .map_err(|e| e.into())
            .and_then(|socket| Ok(socket.framed(Codec::new()).split()))
            .and_then(move |(sink, stream)| {
                sm_client_for_task
                    .request_public_key()
                    .map_err(ChannelError::SecurityModule)
                    .and_then(move |public_key| {
                        let init_channel_active = InitChannelActive {
                            neighbor_public_key: public_key,
                            channel_rand_value: rand_value,
                            channel_index: channel_index,
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
            timeout: Timeout::new(time::Duration::from_secs(5), handle).unwrap(),
            // TODO CR: The Timer module should be the only source of time. We should use the
            // timer module to achieve timeout instead of time::Duration::from_secs. This is
            // important so that we will be able to write tests the simulate the passage of time.
            // 
            // To make a similar type of timer we may be able to get a duplicate Timer stream,
            // and create a future that wait until the duplicate Timer stream ticks some constant amount of times.
            //
            // TODO CR: If we have some magic number like 5, it should be declared as a
            // constant somewhere with a meaningful name. Later we might be able to group all
            // those constants, maybe even allow to configure them.
            rng,
            sm_client: sm_client.clone(),
            remote_public_key: None,
            nw_sender: networker_tx.clone(),
            channel_index: Some(channel_index),
        }
    }

    pub fn remote_public_key(&self) -> PublicKey {
        self.remote_public_key.clone()
    }

    /// Pack and encrypt a message to be sent to remote.
    fn pack_msg(&mut self, msg_type: MessageType, content: Bytes) -> Result<Bytes, ChannelError> {
        let padding_len = self.os_rng.next_u32() % MAX_PADDING_LEN;
        // TODO CR: I think that we should be more careful when calculating padding_len.
        // Note that doing something like (rand() % 0x100) is really random, but doing something
        // like (rand() % 17) is not absolutely random, because the random is not really uniformly
        // distributed over all the 17 possibilities. This is because 17 does not divide 2^32.
        //
        // We can add an assertion somewhere that MAX_PADDING_LEN is a divisor of 2^32, or use some other function.
        //
        // We should check, maybe we have a better interface for generating numbers using SecureRandom? This is not necessary though.
        let mut padding_bytes = [0u8; MAX_PADDING_LEN as usize];
        self.os_rng
            .fill_bytes(&mut padding_bytes[..padding_len as usize]);

        let encrypt_message = EncryptMessage {
            inc_counter: self.send_counter,
            // TODO CR: I realize now that we are using two increasing counters simultaneously: The one
            // in the encryption nonce (EncryptNonceCounter inside sym_encrypt.rs) and inc_counter
            // here. Maybe using EncryptNonceCounter should be enough. If we want to use
            // EncryptNonceCounter we should change the interface of sym_encrypt to allow giving the counter
            // as nonce argument. I am still not sure if doing this is insecure in some way. We need to
            // discuss this.
            rand_padding: Bytes::from(&padding_bytes[..padding_len as usize]),
            message_type: msg_type,
            // TODO CR: I suggest changing msg_type -> message_type
            content: content,
        };

        let encrypted = self.encryptor.encrypt(&encrypt_message.encode()?)?;

        serialize_message(Bytes::from(encrypted))
            .map_err(|e| e.into())
            .and_then(|bytes| {
                // TODO CR: I think that it is surprising that pack_msg increases the send_counter.
                // Maybe we should give the current self.send_counter as argument to pack_msg, and
                // incremenet it outside of pack_msg? See also increasing of counter inside
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
    fn try_start_send_inner(&mut self, msg: ChannelerToNetworker) -> Poll<(), ChannelError> {
        debug_assert!(self.inner_buffered.is_none());

        self.inner_sender
            .start_send(msg)
            .map_err(|_| ChannelError::SendToNetworkerFailed)
            .and_then(|start_send| {
                if let AsyncSink::NotReady(msg) = start_send {
                    self.inner_buffered = Some(msg);
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
            let poll_result = self.inner_receiver
                .poll()
                .map_err(|_| ChannelError::RecvFromInnerFailed);

            if let Some(msg) = try_ready!(poll_result) {
                match msg {
                    ToChannel::TimeTick => {
                        // TODO CR:
                        // Is there any danger here for panic! ?
                        // Is it possible that at this point send_ka_ticks == 0?
                        self.send_ka_ticks -= 1;
                        self.recv_ka_ticks -= 1;

                        if self.recv_ka_ticks == 0 {
                            return Err(ChannelError::KeepAliveTimeout);
                        } else {
                            if self.send_ka_ticks == 0 {
                                self.send_ka_ticks = KEEP_ALIVE_TICKS;

                                let msg = self.pack_msg(MessageType::KeepAlive, Bytes::new())?;
                                return Ok(Async::Ready(Some(msg)));
                            }
                        }
                    }
                    ToChannel::SendMessage(raw) => {
                        let msg = self.pack_msg(MessageType::User, Bytes::from(raw))?;
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
                        self.recv_ka_ticks = 2 * KEEP_ALIVE_TICKS;
                    }
                    MessageType::User => {
                        let msg = ChannelerToNetworker {
                            remote_public_key: self.remote_public_key.clone(),
                            channel_index: self.channel_index,
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

// TODO CR: We might be able to split the file here, as this file is pretty long.
// I'm still not sure about this, as it might be more comfortable to have everything together here.
// What do you think?

// TODO CR:
// I think that we might be able to change this code (impl Future for Channel) to use combinators.
// This might save us all the hassle of the buffered items, and could probably produce shorter
// code. 
//
// What we basically do here is try to get an item from try_poll_inner and push it into send_outer.
// At the same time we try to get an item from poll_outer and push it into send_inner. It's like
// connecting two sinks and streams together.
//
// I think that doing this kind of refactoring here will be worth it, because the code could become
// much easier to reason about.
//
impl Future for Channel {
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
                    if let Some(msg) = self.inner_buffered.take() {
                        try_ready!(self.try_start_send_inner(msg));
                    }
                    match self.try_poll_outer()? {
                        Async::Ready(None) => {
                            debug!("tcp connection closed, closing");
                            return Ok(Async::Ready(()));
                        }
                        Async::Ready(Some(msg)) => {
                            try_ready!(self.try_start_send_inner(msg));
                        }
                        Async::NotReady => {
                            try_ready!(
                                self.inner_sender
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
                Item = (
                    InitChannelActive,
                    InitChannelPassive,
                    FramedSink,
                    FramedStream,
                ),
                Error = ChannelError,
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
                Item = (Exchange, Exchange, DhPrivateKey, FramedSink, FramedStream),
                Error = ChannelError,
            >,
        >,
    ),

    Empty,
}

#[must_use = "futures do nothing unless polled"]
pub struct ChannelNew {
    state: ChannelNewState,
    timeout: Timeout,

    // Utils used in performing exchange
    // TODO CR: We should take a reference to SecureRandom instead.  Possible Rc<R> for 
    // R: SecureRandom.  Also see some previous comments about this.
    rng: SystemRandom,
    sm_client: SecurityModuleClient,
    channel_index: Option<u32>,
    nw_sender: mpsc::Sender<ChannelerToNetworker>, // TODO CR: I suggest to change this into 
                                                   // networker_sender. It was difficult for me to
                                                   // guess what is nw until I saw that the
                                                   // messages type is ChannelerToNetworker.
    remote_public_key: Option<PublicKey>,
}

impl ChannelNew {
    // TODO: If we use RefCell<...> for neighbors, this function could return a boolean value
    // or a Result (for various failure values) instead of an impl Future.
    /// Obtain neighbors structure, and find out whether the remote_public_key may initialize 
    /// Find out whether remote_public_key may actively initiate a channel with index
    /// channel_index.
    fn create_validation_task(
        remote_public_key: PublicKey,
        channel_index: u32,
        // TODO CR: I think that we can be ok here with RefCell<HashMap<...>>, 
        // what do you think? 
        // Do we have a good basis to believe that futures might be spawned implicitly in different
        // threads?
        //
        // In addition, maybe we should alias HashMap<PublicKey, ChannelerNeighbor> to be some
        // type, or maybe give it a type of its own. 
        // I noticed that we copy paste "HashMap<PublicKey, ChannelerNeighbor>" in at least 3
        // places in this file, this means it is probably  important enough to get its own name.
        neighbors: AsyncMutex<HashMap<PublicKey, ChannelerNeighbor>>,
    ) -> impl Future<Item = (), Error = ChannelError> {
        neighbors
            .acquire(move |neighbors| {
                // TODO CR: I think that the whole clause of validation_result = ... should be a
                // separate function. This will make this function (create_validation_task)
                // cleaner.
                let validation_result = if let Some(neighbor) = neighbors.get(&remote_public_key) {
                    // TODO CR: Maybe we should use match instead of if ... .is_some() ?
                    if neighbor.info.socket_addr.is_some() {
                        Err(ChannelError::Closed("not allowed initator"))
                    } else {
                        if channel_index < neighbor.info.max_channels {
                            if neighbor.channels.get(&channel_index).is_some() {
                                Err(ChannelError::Closed("Index is already in use"))
                            } else {
                                Ok(())
                            }
                        } else {
                            Err(ChannelError::Closed("Index is too large"))
                        }
                    }
                } else {
                    Err(ChannelError::Closed("unknown neighbor"))
                };

                if let Err(e) = validation_result {
                    Err((Some(neighbors), e))
                } else {
                    Ok((neighbors, ()))
                }
            })
            .map_err(|e| match e {
                AsyncMutexError::Function(e) => e.into(),
                _ => ChannelError::AsyncMutexError,
            })
    }

    fn create_exchange_task(
        remote_public_key: PublicKey,
        sent_rand_value: RandValue,
        recv_rand_value: RandValue,
        sink: FramedSink,
        stream: FramedStream,
        // TODO CR: We should take here rng: R, where R: SecureRandom.
        rng: &SystemRandom,
        sm_client: &SecurityModuleClient,
    ) -> impl Future<
        Item = (Exchange, Exchange, DhPrivateKey, FramedSink, FramedStream),
        Error = ChannelError,
    > {
        // Generate ephemeral DH private key
        let key_salt = Salt::new(rng);
        let comm_private_key = DhPrivateKey::new(rng);
        let comm_public_key = comm_private_key.compute_public_key();

        // message = (channelRandValue + commPublicKey + keySalt)
        // TODO CR: What is the meaning of 1024 here?
        // Whenever we use a magic number, we should have a const that equals this value with a
        // proper name and some explanation.
        let mut msg = Vec::with_capacity(1024);

        // TODO CR: Could/should we use `msg` of type Bytes instead, like done previously in this
        // source file? Is this done because of the interface of request_sign? We might be able to
        // change it if necessary. What do you think? I am not sure I fully understand the
        // advantages and disadvantages of Vec vs Bytes in this case.
        msg.extend_from_slice(recv_rand_value.as_bytes());
        msg.extend_from_slice(comm_public_key.as_bytes());
        msg.extend_from_slice(key_salt.as_bytes());

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
                        // TODO CR: What is the meaning of 1024 here? See previous comment about
                        // this.
                        let mut msg = Vec::with_capacity(1024);
                        // TODO CR: Maybe we should turn this part of code into a function?
                        msg.extend_from_slice(sent_rand_value.as_bytes());
                        msg.extend_from_slice(recv_exchange.comm_public_key.as_bytes());
                        msg.extend_from_slice(recv_exchange.key_salt.as_bytes());

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
impl Future for ChannelNew {
    type Item = (u32, mpsc::Sender<ToChannel>, Channel);
    type Error = ChannelError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        trace!("ChannelNew::poll - {:?}", ::std::time::Instant::now());

        loop {
            if self.timeout.poll()?.is_ready() {
                return Err(ChannelError::Timeout);
            }

            match mem::replace(&mut self.state, ChannelNewState::Empty) {
                // TODO CR: It happened to me more than once that my code did reach this
                // unreachable!() statement, but I could never see any error, because the error is
                // swallowed when using futures. Do you have an idea for a solution to this
                // problem? A bug of this type happened to me with the exact same pattern
                // (mem::replace() ... unreachable!()) with the implementation of AsyncMutex.
                //
                // I'm not suggesting that we have a bug here, but if we forget the 
                // self.state = ...
                // statement on any arm of the match {} clause, we will have a bug and it could be
                // difficult to spot. What is your opinion?
                ChannelNewState::Empty => unreachable!("invalid state"),
                ChannelNewState::InitChannel(mut init_channel_task) => {
                    match init_channel_task.poll()? {
                        Async::Ready((init_active, init_passive, tx, rx)) => {
                            trace!("ChannelNewState::InitialChannel [Ready]");
                            let remote_public_key: PublicKey;
                            let sent_rand_value: RandValue;
                            let recv_rand_value: RandValue;

                            if self.channel_index.is_some() {
                                remote_public_key = init_passive.neighbor_public_key.clone();
                                sent_rand_value = init_active.channel_rand_value.clone();
                                recv_rand_value = init_passive.channel_rand_value.clone();
                            } else {
                                remote_public_key = init_active.neighbor_public_key.clone();
                                sent_rand_value = init_passive.channel_rand_value.clone();
                                recv_rand_value = init_active.channel_rand_value.clone();

                                self.channel_index = Some(init_active.channel_index);
                            }

                            self.remote_public_key = Some(remote_public_key.clone());

                            let exchange_task = ChannelNew::create_exchange_task(
                                remote_public_key,
                                sent_rand_value,
                                recv_rand_value,
                                tx,
                                rx,
                                &self.rng,
                                &self.sm_client,
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
                    debug_assert!(self.channel_index.is_some());
                    debug_assert!(self.remote_public_key.is_some());

                    match exchange_task.poll()? {
                        // TODO CR: I suggest to rename: 
                        // sent_xchg -> sent_exchange, 
                        // recv_xchg -> recv_exchange,
                        // to conform to the other places we mention the Exchange message.
                        // Also rename my_privete_key -> my_private_key (:
                        Async::Ready((sent_xchg, recv_xchg, my_privete_key, tx, rx)) => {
                            trace!("ChannelNewState::Exchange       [Ready]");
                            // Combine self private key, received public key
                            // and received key salt to derive symmetric key
                            // for sending data
                            let key_send = my_privete_key.derive_symmetric_key(
                                &recv_xchg.comm_public_key,
                                &sent_xchg.key_salt,
                            );

                            // Combine self private key, received public key
                            // and received key salt to derive symmetric key
                            // for receiving data
                            let key_recv = my_privete_key.derive_symmetric_key(
                                &recv_xchg.comm_public_key,
                                &recv_xchg.key_salt,
                            );

                            let encrypt_nonce_counter = EncryptNonceCounter::new(&mut self.rng);

                            let encryptor = Encryptor::new(&key_send, encrypt_nonce_counter);
                            let decryptor = Decryptor::new(&key_recv);

                            let (inner_tx, inner_rx) = mpsc::channel::<ToChannel>(0);

                            // TODO CR: See comment beloew about the debug_assert!-s.
                            // We might be able to design this part in a different way, to avoid
                            // the many expect()-s and assert!s. In the meanwhile, we can make sure
                            // that we do this only once, maybe in the beginning of the function.
                            let channel_index =
                                self.channel_index.take().expect("missing channel index");

                            let remote_public_key = self.remote_public_key
                                .take()
                                .expect("missing remote public key");

                            let channel = Channel {
                                // TODO CR: Use some SecureRandom ref from the outside, maybe Rc<R>
                                // where R: SecureRandom.
                                os_rng: OsRng::new()?,
                                remote_public_key,
                                channel_index,
                                inner_sender: self.nw_sender.clone(),
                                inner_receiver: inner_rx,
                                inner_buffered: None,
                                outer_sender: tx,
                                outer_receiver: rx,
                                outer_buffered: None,
                                send_counter: 0,
                                recv_counter: 0,
                                encryptor: encryptor,
                                decryptor: decryptor,
                                send_ka_ticks: KEEP_ALIVE_TICKS,
                                recv_ka_ticks: 2 * KEEP_ALIVE_TICKS,
                            };

                            return Ok(Async::Ready((channel_index, inner_tx, channel)));
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

#[inline]
/// Cyclic increase of counter.
fn increase_counter(counter: &mut u64) {
    if *counter == u64::max_value() {
        *counter = 0;
    } else {
        *counter += 1;
    }
}

// ===== Error Conversion ===

impl From<io::Error> for ChannelError {
    #[inline]
    fn from(e: io::Error) -> ChannelError {
        ChannelError::Io(e)
    }
}

impl From<SchemaError> for ChannelError {
    #[inline]
    fn from(e: SchemaError) -> ChannelError {
        ChannelError::Schema(e)
    }
}

impl From<CodecError> for ChannelError {
    #[inline]
    fn from(e: CodecError) -> ChannelError {
        ChannelError::Codec(e)
    }
}

impl From<SymEncryptError> for ChannelError {
    #[inline]
    fn from(e: SymEncryptError) -> ChannelError {
        ChannelError::EncryptError(e)
    }
}

impl From<SecurityModuleClientError> for ChannelError {
    #[inline]
    fn from(e: SecurityModuleClientError) -> ChannelError {
        ChannelError::SecurityModule(e)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn gen_channeler_neighbor(byte: u8) -> ChannelerNeighbor {
        let key_bytes = [byte; 32];
        let public_key = PublicKey::from_bytes(&key_bytes).unwrap();

        let info = ChannelerNeighborInfo {
            public_key: public_key,
            socket_addr: None,
            max_channels: 1,
        };

        let neighbor = ChannelerNeighbor {
            info,
            channels: HashMap::new(),
            retry_ticks: 0,
            num_pending: 0,
        };

        neighbor
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
    /// Notation: (Initiator, Smaller, In used)
    #[test]
    fn test_validation() {
        let (keys, mut neighbors) = gen_neighbors();

        let (tx, _) = mpsc::channel::<ToChannel>(0);
        {
            let mut neighbor = neighbors.get_mut(&keys[2]).unwrap();
            neighbor.channels.insert(0, tx);
        }

        let neighbors = AsyncMutex::new(neighbors);

        // Case 1: (Y, Y, N)
        //
        // - The received key belong to a neighbor that has the initiator role.
        // - The channel index smaller than the maximum allowed channels and this
        //   channel index is not already in used.
        let remote_public_key = keys[1].clone();
        let channel_index = 0;
        let validation_task =
            ChannelNew::create_validation_task(remote_public_key, channel_index, neighbors.clone());

        assert!(validation_task.wait().is_ok());

        // Case 2: (Y, Y, Y)
        //
        // - The received key belong to a neighbor that has the initiator role.
        // - The channel index smaller than the maximum allowed channels but this
        //   channel index is already in used.
        let remote_public_key = keys[2].clone();
        let channel_index = 0;
        let validation_task =
            ChannelNew::create_validation_task(remote_public_key, channel_index, neighbors.clone());

        assert!(validation_task.wait().is_err());

        // Case 3: (Y, N, X)
        //
        // - The received key belong to a neighbor that has the initiator role.
        // - The channel index greater or equal than the maximum allowed channels.
        let remote_public_key = keys[1].clone();
        let channel_index = 1;
        let validation_task =
            ChannelNew::create_validation_task(remote_public_key, channel_index, neighbors.clone());

        assert!(validation_task.wait().is_err());

        // Case 2: (N, X, X)
        //
        // - The received key belong to a neighbor that doesn't have the initiator role.
        // - The channel index smaller than the maximum allowed channels and this
        //   channel index is not already in used.
        let remote_public_key = keys[3].clone();
        let channel_index = 100;
        let validation_task =
            ChannelNew::create_validation_task(remote_public_key, channel_index, neighbors.clone());

        assert!(validation_task.wait().is_err());
    }
}
