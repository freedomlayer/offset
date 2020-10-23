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
extern crate common;

mod connect_pool;
mod inner_loop;
// mod connector_utils;
mod channeler;
mod listen_pool;
mod listen_pool_state;
mod overwrite_channel;
mod types;

pub use self::channeler::{channeler_loop, SpawnChannelerError};
pub use self::inner_loop::ChannelerError;
