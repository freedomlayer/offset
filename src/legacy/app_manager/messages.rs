#![allow(unused)]

use std::net::SocketAddr;

use futures::sync::oneshot;

use crypto::identity::PublicKey;

use networker::messages::{NeighborStatus, RequestPath};
use networker::messenger::types::NeighborsRoute;

use funder::messages::{FriendInfo, FriendRequestsStatus, FriendStatus, RequestSendFunds,
                        FriendsRouteWithCapacity};
use proto::networker::{ChannelToken, NetworkerSendPrice};
use proto::funder::FunderSendPrice;


#[derive(Clone)]
pub struct SetNeighborRemoteMaxDebt {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub remote_max_debt: u64,
}

#[derive(Clone)]
pub struct ResetNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub current_token: ChannelToken,
}

#[derive(Clone)]
pub struct SetNeighborMaxChannels {
    pub neighbor_public_key: PublicKey,
    pub max_channels: u16,
}

#[derive(Clone)]
pub struct SetNeighborAddr {
    pub neighbor_public_key: PublicKey,
    pub neighbor_addr: Option<SocketAddr>,
}


#[derive(Clone)]
pub struct AddNeighbor {
    pub neighbor_public_key: PublicKey,
    pub neighbor_addr: Option<SocketAddr>,
    pub max_channels: u16, // Maximum amount of token channels
}

#[derive(Clone)]
pub struct RemoveNeighbor {
    pub neighbor_public_key: PublicKey,
}

#[derive(Clone)]
pub struct SetNeighborIncomingPathFee {
    pub neighbor_public_key: PublicKey,
    pub incoming_path_fee: u64,
}

#[derive(Clone)]
pub struct SetNeighborStatus {
    pub neighbor_public_key: PublicKey,
    pub status: NeighborStatus,
}

#[derive(Clone)]
pub struct OpenNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
    pub send_price: NetworkerSendPrice,
}

#[derive(Clone)]
pub struct CloseNeighborChannel {
    pub neighbor_public_key: PublicKey,
    pub channel_index: u16,
}


pub enum NetworkerCommand {
    AddNeighbor(AddNeighbor),
    RemoveNeighbor(RemoveNeighbor),
    OpenNeighborChannel(OpenNeighborChannel),
    CloseNeighborChannel(CloseNeighborChannel),
    SetNeighborIncomingPathFee(SetNeighborIncomingPathFee),
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

pub struct AddFriend {
    friend_public_key: PublicKey,
    remote_max_debt: u128,
}

pub struct RemoveFriend {
    friend_public_key: PublicKey,
}

pub struct OpenFriend {
    friend_public_key: PublicKey,
    send_price: FunderSendPrice,
}

pub struct CloseFriend {
    friend_public_key: PublicKey,
}

pub struct EnableFriend {
    friend_public_key: PublicKey,
}

pub struct DisableFriend {
    friend_public_key: PublicKey,
}

pub struct SetFriendRemoteMaxDebt {
    friend_public_key: PublicKey,
    remote_max_debt: u128,
}

pub struct ResetFriendChannel {
    friend_public_key: PublicKey,
    current_token: ChannelToken,
    balance_for_reset: u128,
}


pub enum AppManagerToFunder {
    RequestSendFunds(RequestSendFunds),
    AddFriend(AddFriend),
    RemoveFriend(RemoveFriend),
    OpenFriend(OpenFriend),
    CloseFriend(CloseFriend),
    EnableFriend(EnableFriend),
    DisableFriend(DisableFriend),
    SetFriendRemoteMaxDebt(SetFriendRemoteMaxDebt),
    ResetFriendChannel(ResetFriendChannel),
}

pub struct ResponseNeighborsRoute {
    routes: Vec<NeighborsRoute>,
}

pub struct RequestNeighborsRoute {
    source_node_public_key: PublicKey,
    dest_node_public_key: PublicKey,
    response_sender: oneshot::Sender<ResponseNeighborsRoute>,
}

pub struct RouteDirect {
    source_public_key: PublicKey,
    dest_public_key: PublicKey,
}

pub struct RouteLoopFromFriend {
    friend_public_key: PublicKey,
}

pub struct RouteLoopToFriend {
    friend_public_key: PublicKey,
}

pub enum RouteRequest {
    Direct(RouteDirect),
    LoopFromFriend(RouteLoopFromFriend),
    LoopToFriend(RouteLoopToFriend),
}

pub struct ResponseFriendsRoute {
    routes: Vec<FriendsRouteWithCapacity>,

}

pub struct RequestFriendsRoute {
    route_request: RouteRequest,
    pub response_sender: oneshot::Sender<ResponseFriendsRoute>,
}

