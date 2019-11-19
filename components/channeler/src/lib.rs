#![crate_type = "lib"]
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
extern crate common;

mod channeler;
mod connect_pool;
// mod connector_utils;
mod listen_pool;
mod listen_pool_state;
mod overwrite_channel;
mod spawn;
mod types;

pub use self::channeler::ChannelerError;
pub use self::spawn::{spawn_channeler, SpawnChannelerError};
