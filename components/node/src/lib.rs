#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde;

#[macro_use]
extern crate quickcheck_derive;

mod node;
mod types;

pub use self::node::{node, NodeError};
pub use self::types::{NodeConfig, NodeMutation, NodeState};
pub use app_server::{ConnPairServer, IncomingAppConnection};
