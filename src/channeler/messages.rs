use bytes::Bytes;
use crypto::identity::PublicKey;

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
