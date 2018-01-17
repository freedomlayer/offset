use bytes::Bytes;

use crypto::dh::{DhPublicKey, Salt};
use crypto::identity::{PublicKey, Signature};
use crypto::rand_values::RandValue;

pub use proto::channeler::MessageType;

// TODO CR: Maybe this should have the type usize?
pub const MAX_PADDING_LEN: u32 = 32;

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
