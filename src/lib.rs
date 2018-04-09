#![crate_type = "lib"]
#![feature(nll)]
#![feature(try_from)]
#![feature(refcell_replace_swap)]
#![feature(iterator_step_by)]
#![cfg_attr(feature = "dev", feature(plugin))]
#![cfg_attr(feature = "dev", plugin(clippy))]
#![cfg_attr(not(feature = "dev"), allow(unknown_lints))]
#![allow(needless_pass_by_value)]

extern crate byteorder;
extern crate bytes;
extern crate capnp;
extern crate crossbeam;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate log;
extern crate rand;
extern crate ring;
extern crate rusqlite;
extern crate tokio_core;
extern crate tokio_io;

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
use proto::proto_impl::indexer::indexer_capnp;
use proto::proto_impl::channeler::channeler_capnp;
