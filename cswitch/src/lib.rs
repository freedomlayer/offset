#![crate_type = "lib"]
#![feature(nll)]
#![feature(i128_type)]
#![feature(use_nested_groups)]
#![feature(refcell_replace_swap)]
#![feature(conservative_impl_trait, universal_impl_trait)]
#![feature(iterator_step_by)]
#![cfg_attr(feature = "dev", feature(plugin))]
#![cfg_attr(feature = "dev", plugin(clippy))]
#![cfg_attr(not(feature = "dev"), allow(unknown_lints))]
#![allow(needless_pass_by_value)]
#![feature(test)]

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

// Utils
pub mod utils;

//mod inner_messages;

// Modules
pub mod timer;
pub mod indexer;
pub mod security;
//pub mod database;
pub mod channeler;
pub mod networker;
pub mod app_manager;

// Schemas
mod schema;
use schema::common_capnp;
use schema::indexer_capnp;
use schema::channeler_capnp;
