#![crate_type = "lib"]

#![feature(nll)]
#![feature(try_from)]
#![feature(iterator_step_by)]
#![feature(refcell_replace_swap)]
#![cfg_attr(not(feature = "cargo-clippy"), allow(unknown_lints))]
#![cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]

extern crate byteorder;
extern crate bytes;
extern crate capnp;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
extern crate rand;
extern crate ring;
extern crate rusqlite;
extern crate tokio_core;
extern crate tokio_io;
extern crate async_mutex;

#[cfg(test)]
extern crate num_traits;


#[macro_use]
pub mod utils;
pub mod crypto;

// Modules
pub mod app_manager;
pub mod channeler;
pub mod database;
pub mod funder;
pub mod indexer_client;
pub mod networker;
pub mod security_module;
pub mod timer;


mod proto;
// FIXME: The capnpc generated code assumes that we
// add it as a module at the top level of the crate.
use proto::proto_impl::common::common_capnp;
// use proto::proto_impl::indexer::indexer_capnp;
use proto::proto_impl::channeler::channeler_capnp;
