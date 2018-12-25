use std::collections::HashMap;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use crate::index_server::messages::{RequestRoutes, 
    RouteWithCapacity, IndexMutation};


#[derive(Debug, Clone)]
pub struct IndexClientState {
    pub friends: HashMap<PublicKey, (u128, u128)>,
}

#[derive(Debug, Clone)]
pub enum IndexClientMutation {
    // UpdateSendCapacity((PublicKey, u128)),
    // UpdateRecvCapacity((PublicKey, u128)),
    UpdateFriend((PublicKey, (u128, u128))),
    RemoveFriend(PublicKey),
}

// ---------------------------------------------------
// IndexClient <--> AppServer communication
// ---------------------------------------------------


#[derive(Debug)]
/// ISA stands for Index Server Address
pub struct IndexClientReport<ISA> {
    /// A list of trusted index servers.
    index_servers: Vec<ISA>,
    /// The server we are currently connected to (None if not connected).
    connected_server: Option<ISA>,
}

#[derive(Debug)]
pub enum IndexClientReportMutation<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
    SetConnectedServer(Option<ISA>),
}

#[derive(Debug)]
pub enum ResponseRoutesResult {
    Success(Vec<RouteWithCapacity>),
    Failure,
}

#[derive(Debug)]
pub struct ClientResponseRoutes {
    pub request_id: Uid,
    pub result: ResponseRoutesResult,
}

#[derive(Debug)]
pub enum IndexClientToAppServer<ISA> {
    ReportMutations(Vec<IndexClientReportMutation<ISA>>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug)]
pub enum AppServerToIndexClient<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
    RequestRoutes(RequestRoutes),
    ApplyMutations(Vec<IndexClientMutation>),
}

