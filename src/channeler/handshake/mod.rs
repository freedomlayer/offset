use ring::aead::{SealingKey, OpeningKey};

use crypto::identity::PublicKey;
use proto::channeler::ChannelId;

mod error;
mod utils;
mod server;
mod client;

#[cfg(test)]
mod tests;

pub use self::error::HandshakeError;
pub use self::client::HandshakeClient;
pub use self::server::HandshakeServer;

pub struct ChannelMetadata {
    pub remote_public_key: PublicKey,

    pub tx_cid: ChannelId,
    pub tx_key: SealingKey,

    pub rx_cid: ChannelId,
    pub rx_key: OpeningKey,
}

