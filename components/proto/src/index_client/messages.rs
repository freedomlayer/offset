use std::collections::HashMap;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use crate::index_server::messages::RouteWithCapacity;
pub use crate::index_server::messages::{RequestRoutes, IndexMutation, UpdateFriend};


#[derive(Debug, Clone)]
pub struct IndexClientState {
    pub friends: HashMap<PublicKey, (u128, u128)>,
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
pub enum ResponseRoutesResult<P> {
    Success(Vec<RouteWithCapacity<P>>),
    Failure,
}

#[derive(Debug)]
pub struct ClientResponseRoutes<P> {
    pub request_id: Uid,
    pub result: ResponseRoutesResult<P>,
}

#[derive(Debug)]
pub enum IndexClientToAppServer<ISA,P> {
    ReportMutations(Vec<IndexClientReportMutation<ISA>>),
    ResponseRoutes(ClientResponseRoutes<P>),
}

#[derive(Debug)]
pub enum AppServerToIndexClient<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
    RequestRoutes(RequestRoutes),
    ApplyMutations(Vec<IndexMutation>),
}
