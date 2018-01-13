use bytes::Bytes;
use utils::crypto::identity::PublicKey;
use channeler::types::ChannelerNeighborInfo;

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
    RemoveNeighbor { neighbor_public_key: PublicKey },
    /// Request to set the maximum amount of token channel.
    SetMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
}
