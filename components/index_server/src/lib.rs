#![crate_type = "lib"]
#![feature(async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![feature(map_get_key_value)]
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

extern crate common;

#[allow(unused)]
mod backoff_connector;
#[allow(unused)]
mod graph;
// mod net_server;
// mod server;
// mod verifier;

// pub use net_server::{net_index_server, NetIndexServerError};
