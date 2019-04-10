#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

mod adapters;
pub mod connect;
mod net_node;
mod node;
mod types;

pub use self::net_node::{net_node, NetNodeError};
pub use self::types::{NodeConfig, NodeState};
pub use app_server::IncomingAppConnection;
