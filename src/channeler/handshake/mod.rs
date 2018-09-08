use ring::aead::{SealingKey, OpeningKey};

use crypto::identity::PublicKey;
use proto::channeler::ChannelId;

mod error;
mod helpers;
mod server;
mod client;

#[cfg(test)]
mod tests;

pub use self::error::HandshakeError;
pub use self::client::HandshakeClient;
pub use self::server::HandshakeServer;

pub struct ChannelMetadata {
    pub remote_public_key: PublicKey,

    pub tx_channel_id: ChannelId,
    pub tx_sealing_key: SealingKey,

    pub rx_channel_id: ChannelId,
    pub rx_opening_key: OpeningKey,
}

