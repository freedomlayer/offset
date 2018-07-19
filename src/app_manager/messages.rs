use std::net::SocketAddr;

use crypto::identity::PublicKey;

use networker::messages::{NeighborStatus, RequestPath};

use funder::messages::{FriendInfo, FriendRequestsStatus, FriendStatus, RequestSendFunds};
use proto::networker::{ChannelToken, NetworkerSendPrice};


#[allow(dead_code)]
#[derive(Clone)]
pub struct SetNeighborRemoteMaxDebt {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub remote_max_debt: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct ResetNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub current_token: ChannelToken,
    pub balance_for_reset: i64,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct SetNeighborMaxChannels {
    pub neighbor_public_key: PublicKey,
    pub max_channels: u16,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct SetNeighborAddr {
    pub neighbor_public_key: PublicKey,
    pub neighbor_addr: Option<SocketAddr>,
}


#[allow(dead_code)]
#[derive(Clone)]
pub struct AddNeighbor {
    pub neighbor_public_key: PublicKey,
    pub neighbor_socket_addr: Option<SocketAddr>,
    pub max_channels: u16, // Maximum amount of token channels
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct RemoveNeighbor {
    pub neighbor_public_key: PublicKey,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct SetNeighborStatus {
    pub neighbor_public_key: PublicKey,
    pub status: NeighborStatus,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct OpenNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub send_price: NetworkerSendPrice,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct CloseNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
}


#[allow(dead_code)]
pub enum NetworkerCommand {
    AddNeighbor(AddNeighbor),
    RemoveNeighbor(RemoveNeighbor),
    OpenNeighborChannel(OpenNeighborChannel),
    CloseNeighborChannel(CloseNeighborChannel),
    SetNeighborStatus(SetNeighborStatus),
    SetNeighborRemoteMaxDebt(SetNeighborRemoteMaxDebt),
    SetNeighborMaxChannels(SetNeighborMaxChannels),
    SetNeighborAddr(SetNeighborAddr),
    ResetNeighborChannel(ResetNeighborChannel),
}

pub enum AppManagerToNetworker {
    RequestPath(RequestPath),
    NetworkerCommand(NetworkerCommand),
}

/*
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
*/

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
