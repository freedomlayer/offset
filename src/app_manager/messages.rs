use std::net::SocketAddr;

use crypto::identity::PublicKey;

use networker::messages::{NeighborStatus, RequestPath};

use funder::messages::{FriendInfo, FriendRequestsStatus, FriendStatus, RequestSendFunds};

use proto::indexer::IndexingProviderId;
use indexer_client::messages::{IndexingProviderInfo, IndexingProviderStatus, 
    RequestFriendsRoutes, RequestNeighborsRoutes};

pub enum AppManagerToNetworker {
    RequestPath(RequestPath),
    SetNeighborWantedRemoteMaxDebt {
        neighbor_public_key: PublicKey,
        wanted_remote_max_debt: u64,
    },
    ResetNeighborChannel {
        neighbor_public_key: PublicKey,
        channel_index: u32,
        // TODO: Should we add wanted parameters for the ChannelReset,
        // or let the Networker use the last Inconsistency message information
        // to perform Reset?
    },
    SetNeighborMaxChannels {
        neighbor_public_key: PublicKey,
        max_channels: u32,
    },
    AddNeighbor {
        neighbor_public_key: PublicKey,
        neighbor_socket_addr: Option<SocketAddr>,
        max_channels: u32, // Maximum amount of token channels
        wanted_remote_max_debt: u64,
    },
    RemoveNeighbor {
        neighbor_public_key: PublicKey,
    },
    SetNeighborStatus {
        neighbor_public_key: PublicKey,
        status: NeighborStatus,
    },
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
    SetFriendWantedRemoteMaxDebt {
        friend_public_key: PublicKey,
        wanted_remote_max_debt: u128,
    },
}
