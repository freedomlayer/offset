#![crate_type = "lib"]
#![feature(refcell_replace_swap)]
#![feature(conservative_impl_trait)]
#![feature(drain_filter, iterator_step_by)]
#![cfg_attr(feature = "dev", feature(plugin))]
#![cfg_attr(feature = "dev", plugin(clippy))]
#![cfg_attr(not(feature = "dev"), allow(unknown_lints))]
#![allow(needless_pass_by_value)]

#[macro_use]
extern crate log;
extern crate ring;
extern crate rand;
extern crate bytes;
extern crate capnp;
extern crate crossbeam;
extern crate futures;
extern crate byteorder;
extern crate tokio_core;
extern crate tokio_io;
extern crate futures_mutex;

pub mod crypto;

pub mod inner_messages;
// mod networker_state_machine;

pub mod close_handle;
pub mod security_module;
pub mod channeler;

pub mod async_mutex;
mod service_client;
pub mod timer;

mod schema;
use schema::channeler_capnp;
