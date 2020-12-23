#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate quickcheck_derive;

// Workaround for issue: https://github.com/rust-lang/rust/issues/64450
extern crate offset_mutual_from as mutual_from;

pub mod app_server;
pub mod consts;
pub mod crypto;
pub mod file;
pub mod funder;
pub mod index_client;
pub mod index_server;
pub mod keepalive;
pub mod net;
pub mod relay;
// pub mod report;
pub mod secure_channel;
pub mod ser_string;
