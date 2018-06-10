use std::net::SocketAddr;

use crypto::identity::PublicKey;

use networker::messages::{NeighborStatus, RequestPath};

use funder::messages::{FriendInfo, FriendRequestsStatus, FriendStatus, RequestSendFunds};
use proto::indexer::IndexingProviderId;
use proto::networker::ChannelToken;

use indexer_client::messages::{IndexingProviderInfo, IndexingProviderStatus, 
    RequestFriendsRoutes, RequestNeighborsRoutes};

#[allow(dead_code)]
pub struct SetNeighborRemoteMaxDebt {
    pub neighbor_public_key: PublicKey,
    pub remote_max_debt: u64,
}

#[allow(dead_code)]
pub struct ResetNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u32,
    pub current_token: ChannelToken,
    pub balance_for_reset: i64,
}

#[allow(dead_code)]
pub struct SetNeighborMaxChannels {
    pub neighbor_public_key: PublicKey,
    pub max_channels: u32,
}

#[allow(dead_code)]
pub struct AddNeighbor {
    neighbor_public_key: PublicKey,
    neighbor_socket_addr: Option<SocketAddr>,
    max_channels: u32, // Maximum amount of token channels
    remote_max_debt: u64,
}

#[allow(dead_code)]
pub struct RemoveNeighbor {
    neighbor_public_key: PublicKey,
}

#[allow(dead_code)]
pub struct SetNeighborStatus {
    neighbor_public_key: PublicKey,
    status: NeighborStatus,
}

#[allow(dead_code)]
pub enum NetworkerConfig {
    SetNeighborRemoteMaxDebt(SetNeighborRemoteMaxDebt),
    ResetNeighborChannel(ResetNeighborChannel),
    SetNeighborMaxChannels(SetNeighborMaxChannels),
    AddNeighbor(AddNeighbor),
    RemoveNeighbor(RemoveNeighbor),
    SetNeighborStatus(SetNeighborStatus),
}

pub enum AppManagerToNetworker {
    RequestPath(RequestPath),
    NetworkerConfig(NetworkerConfig),
}

pub enum AppManagerToIndexerClient {
    AddIndexingProvider(IndexingProviderInfo),
    SetIndexingProviderStatus {
        id: IndexingProviderId,
        status: IndexingProviderStatus,
    },
    RemoveIndexingProvider {
        id: IndexingProviderId,
    },
    RequestNeighborsRoutes(RequestNeighborsRoutes),
    RequestFriendsRoutes(RequestFriendsRoutes),
}

pub enum AppManagerToFunder {
    RequestSendFunds(RequestSendFunds),
    ResetFriendChannel {
        friend_public_key: PublicKey,
    },
    AddFriend {
        friend_info: FriendInfo,
    },
    RemoveFriend {
        friend_public_key: PublicKey,
    },
    SetFriendStatus {
        friend_public_key: PublicKey,
        status: FriendStatus,
        requests_status: FriendRequestsStatus,
    },
    SetFriendRemoteMaxDebt {
        friend_public_key: PublicKey,
        remote_max_debt: u128,
    },
}
