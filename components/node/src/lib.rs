#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]


// #[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

mod types;
mod adapters;
mod node;

pub use self::node::{node, NodeError};
pub use self::types::{NodeConfig, NodeState};
pub use app_server::{AppPermissions, IncomingAppConnection};
