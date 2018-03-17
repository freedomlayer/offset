use bytes::Bytes;

use crypto::identity::PublicKey;

pub const MAX_PADDING_LEN: usize = 32;

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
