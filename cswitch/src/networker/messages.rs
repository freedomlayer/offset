use bytes::Bytes;
use crypto::identity::PublicKey;

/// The internal message sent from `Networker` to `Channeler`.
pub enum NetworkerToChanneler {
    /// Request to send a message via given `Channel`.
    SendChannelMessage {
        neighbor_public_key: PublicKey,
        channel_index: u32,
        content: Bytes,
    },
    /// Request to add a new neighbor.
    AddNeighbor {
        neighbor_info: ChannelerNeighborInfo,
    },
    /// Request to delete a neighbor.
    RemoteNeighbor {
        neighbor_public_key: PublicKey,
    },
    /// Request to set the maximum amount of token channel.
    SetMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    }
}