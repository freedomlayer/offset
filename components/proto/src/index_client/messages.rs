use std::collections::HashMap;

use crypto::uid::Uid;
use crypto::identity::PublicKey;

use crate::index_server::messages::{RouteWithCapacity, NamedIndexServer};
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
    pub index_servers: Vec<NamedIndexServer<ISA>>,
    /// The server we are currently connected to (None if not connected).
    pub opt_connected_server: Option<PublicKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddIndexServer<ISA> {
    pub public_key: PublicKey,
    pub address: ISA,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddIndexServerReport<ISA> {
    pub public_key: PublicKey,
    pub address: ISA,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientReportMutation<ISA> {
    AddIndexServer(AddIndexServerReport<ISA>),
    RemoveIndexServer(PublicKey),
    SetConnectedServer(Option<PublicKey>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseRoutesResult {
    Success(Vec<RouteWithCapacity>),
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    AddIndexServer(AddIndexServer<ISA>),
    RemoveIndexServer(PublicKey),
    RequestRoutes(RequestRoutes),
    ApplyMutations(Vec<IndexMutation>),
}


impl<ISA> IndexClientReport<ISA> 
where
    ISA: Eq + Clone,
{
    pub fn mutate(&mut self, mutation: &IndexClientReportMutation<ISA>) {
        match mutation {
            IndexClientReportMutation::AddIndexServer(add_index_server) => {
                // Remove first, to avoid duplicates:
                self.index_servers.retain(|index_server| 
                                          index_server.public_key != add_index_server.public_key);
                self.index_servers.push(NamedIndexServer {
                    public_key: add_index_server.public_key.clone(),
                    address: add_index_server.address.clone(),
                    name: add_index_server.name.clone(),
                });
            },
            IndexClientReportMutation::RemoveIndexServer(public_key) => {
                self.index_servers.retain(|index_server| 
                                          &index_server.public_key != public_key);
            },
            IndexClientReportMutation::SetConnectedServer(opt_public_key) => {
                self.opt_connected_server = opt_public_key.clone();
            },
        }
    }
}
