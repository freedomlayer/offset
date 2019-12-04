#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate common;

#[macro_use]
extern crate log;

#[allow(unused)]
mod compact_node;

mod messages;
mod server_loop;
#[allow(unused)]
mod store;
