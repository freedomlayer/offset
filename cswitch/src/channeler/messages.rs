use bytes::Bytes;

use utils::crypto::dh::{DhPublicKey, Salt};
use utils::crypto::identity::{PublicKey, Signature};
use utils::crypto::rand_values::RandValue;

pub use schema::channeler_capnp::MessageType;

// TODO CR: Maybe this should have the type usize?
pub const MAX_PADDING_LEN: u32 = 32;

// ===== Internal interfaces =====

/// The internal message expected to be send to a `Channel`.
#[derive(Debug)]
pub enum ToChannel {
    /// A time tick event.
    TimeTick,

    /// Request the `Channel` to send a message.
    SendMessage(Bytes),
}

/// The channel event expected to be sent to `Networker`.
pub enum ChannelEvent {
    /// The `Channel` opened.
    Opened,

    /// All `Channel` closed.
    Closed,

    /// A message received from remote.
    Message(Bytes),
}

/// The internal message expected to be sent to `Networker`.
pub struct ChannelerToNetworker {
    /// The public key of the event sender.
    pub remote_public_key: PublicKey,

    /// The channel index of the event sender.
    pub channel_index: u32,

    /// The event happened.
    pub event: ChannelEvent,
}

// ===== External interfaces =====

/// The message intend to be sent by the active end.
#[derive(PartialEq)]
pub struct InitChannelActive {
    /// The identity public key of the sender of this message.
    pub neighbor_public_key: PublicKey,
    /// An initial random value.
    pub channel_rand_value: RandValue,
    /// The index of this channel.
    pub channel_index: u32,
}

/// The message intend to be sent by the passive end.
#[derive(PartialEq)]
pub struct InitChannelPassive {
    /// The identity public key of the sender of this message.
    pub neighbor_public_key: PublicKey,
    /// An initial random value.
    pub channel_rand_value: RandValue,
}

/// The message used in key exchange.
#[derive(PartialEq)]
pub struct Exchange {
    /// Communication public key.
    pub comm_public_key: DhPublicKey,
    /// A salt for the generation of a shared symmertic encryption key.
    pub key_salt: Salt,
    /// Signature over `(channelRandValue || commPublicKey || keySalt)`
    pub signature: Signature,
}

#[derive(PartialEq)]
pub struct EncryptMessage {
    pub inc_counter: u64,
    pub rand_padding: Bytes,
    pub message_type: MessageType,
    pub content: Bytes,
}
