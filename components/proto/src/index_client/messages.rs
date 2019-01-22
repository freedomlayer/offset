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


#[derive(Debug, Clone, PartialEq, Eq)]
/// ISA stands for Index Server Address
pub struct IndexClientReport<ISA> {
    /// A list of trusted index servers.
    pub index_servers: Vec<ISA>,
    /// The server we are currently connected to (None if not connected).
    pub opt_connected_server: Option<ISA>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientReportMutation<ISA> {
    AddIndexServer(ISA),
    RemoveIndexServer(ISA),
    SetConnectedServer(Option<ISA>),
}

#[derive(Debug, Clone)]
pub enum ResponseRoutesResult {
    Success(Vec<RouteWithCapacity>),
    Failure,
}

#[derive(Debug, Clone)]
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
    ApplyMutations(Vec<IndexMutation>),
}


impl<ISA> IndexClientReport<ISA> 
where
    ISA: Eq + Clone,
{
    pub fn mutate(&mut self, mutation: &IndexClientReportMutation<ISA>) {
        match mutation {
            IndexClientReportMutation::AddIndexServer(address) => {
                // Remove first, to avoid duplicates:
                self.index_servers.retain(|cur_address| cur_address != address);
                self.index_servers.push(address.clone());
            },
            IndexClientReportMutation::RemoveIndexServer(address) => {
                self.index_servers.retain(|cur_address| cur_address != address);
            },
            IndexClientReportMutation::SetConnectedServer(opt_address) => {
                self.opt_connected_server = opt_address.clone();
            },
        }
    }
}
