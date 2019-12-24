#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

// #[macro_use]
extern crate log;

// #[macro_use]
extern crate common;

#[allow(unused)]
mod backoff_connector;
#[allow(unused)]
mod graph;
// mod server;
// mod server_loop;
#[allow(unused)]
mod verifier;

// pub use server::{index_server, IndexServerError};
