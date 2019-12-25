use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use crate::crypto::{PublicKey, Uid};
use crate::funder::messages::{Currency, Rate};
pub use crate::index_server::messages::{
    IndexMutation, RemoveFriendCurrency, RequestRoutes, UpdateFriendCurrency,
};
use crate::index_server::messages::{MultiRoute, NamedIndexServerAddress};
use crate::net::messages::NetAddress;

// TODO: Possibly rename to something more meaningful?
/// Capacity through a (PublicKey, Currency) channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendInfo {
    pub recv_capacity: u128,
    pub rate: Rate,
}

#[derive(Debug, Clone)]
pub struct IndexClientState {
    pub friends: HashMap<(PublicKey, Currency), FriendInfo>,
}

// ---------------------------------------------------
// IndexClient <--> AppServer communication
// ---------------------------------------------------

#[capnp_conv(crate::report_capnp::index_client_report::opt_connected_server)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptConnectedServer {
    PublicKey(PublicKey),
    Empty,
}

// TODO: Replace with a macro:
impl From<Option<PublicKey>> for OptConnectedServer {
    fn from(opt: Option<PublicKey>) -> Self {
        match opt {
            Some(public_key) => OptConnectedServer::PublicKey(public_key),
            None => OptConnectedServer::Empty,
        }
    }
}

impl From<OptConnectedServer> for Option<PublicKey> {
    fn from(opt: OptConnectedServer) -> Self {
        match opt {
            OptConnectedServer::PublicKey(public_key) => Some(public_key),
            OptConnectedServer::Empty => None,
        }
    }
}

#[capnp_conv(crate::report_capnp::index_client_report)]
#[derive(Debug, Clone, PartialEq, Eq)]
/// ISA stands for Index Server Address
pub struct IndexClientReport<ISA = NetAddress> {
    /// A list of trusted index servers.
    pub index_servers: Vec<NamedIndexServerAddress<ISA>>,
    /// The server we are currently connected to (None if not connected).
    #[capnp_conv(with = OptConnectedServer)]
    pub opt_connected_server: Option<PublicKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddIndexServer<ISA> {
    pub public_key: PublicKey,
    pub address: ISA,
    pub name: String,
}

#[capnp_conv(crate::report_capnp::index_client_report_mutation::set_connected_server)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetConnectedServer {
    PublicKey(PublicKey),
    Empty,
}

// TODO: Replace with a macro:
impl From<Option<PublicKey>> for SetConnectedServer {
    fn from(opt: Option<PublicKey>) -> Self {
        match opt {
            Some(public_key) => SetConnectedServer::PublicKey(public_key),
            None => SetConnectedServer::Empty,
        }
    }
}

impl From<SetConnectedServer> for Option<PublicKey> {
    fn from(opt: SetConnectedServer) -> Self {
        match opt {
            SetConnectedServer::PublicKey(public_key) => Some(public_key),
            SetConnectedServer::Empty => None,
        }
    }
}

#[capnp_conv(crate::report_capnp::index_client_report_mutation)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientReportMutation<ISA = NetAddress> {
    AddIndexServer(NamedIndexServerAddress<ISA>),
    RemoveIndexServer(PublicKey),
    #[capnp_conv(with = SetConnectedServer)]
    SetConnectedServer(Option<PublicKey>),
}

#[capnp_conv(crate::app_server_capnp::response_routes_result)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseRoutesResult {
    Success(Vec<MultiRoute>),
    Failure,
}

#[capnp_conv(crate::app_server_capnp::client_response_routes)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResponseRoutes {
    pub request_id: Uid,
    pub result: ResponseRoutesResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexClientReportMutations<ISA> {
    pub opt_app_request_id: Option<Uid>,
    pub mutations: Vec<IndexClientReportMutation<ISA>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientToAppServer<ISA> {
    ReportMutations(IndexClientReportMutations<ISA>),
    ResponseRoutes(ClientResponseRoutes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientRequest<ISA> {
    AddIndexServer(NamedIndexServerAddress<ISA>),
    RemoveIndexServer(PublicKey),
    RequestRoutes(RequestRoutes),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppServerToIndexClient<ISA> {
    AppRequest((Uid, IndexClientRequest<ISA>)), // (app_request_id, app_request)
    ApplyMutations(Vec<IndexMutation>),
}

// TODO: Move this code somewhere else?
impl<ISA> IndexClientReport<ISA>
where
    ISA: Eq + Clone,
{
    pub fn mutate(&mut self, mutation: &IndexClientReportMutation<ISA>) {
        match mutation {
            IndexClientReportMutation::AddIndexServer(add_index_server) => {
                // Remove first, to avoid duplicates:
                self.index_servers
                    .retain(|index_server| index_server.public_key != add_index_server.public_key);
                self.index_servers.push(NamedIndexServerAddress {
                    public_key: add_index_server.public_key.clone(),
                    address: add_index_server.address.clone(),
                    name: add_index_server.name.clone(),
                });
            }
            IndexClientReportMutation::RemoveIndexServer(public_key) => {
                self.index_servers
                    .retain(|index_server| &index_server.public_key != public_key);
            }
            IndexClientReportMutation::SetConnectedServer(opt_public_key) => {
                self.opt_connected_server = opt_public_key.clone();
            }
        }
    }
}
