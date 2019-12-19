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

mod file_trusted_apps;
mod node;
pub mod stnodelib;

pub mod stindexlib;
pub mod stmgrlib;
pub mod strelaylib;

pub use crate::node::{net_node, NetNodeError};
