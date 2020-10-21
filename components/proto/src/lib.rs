#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
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

#[macro_use]
pub mod macros;
pub mod app_server;
pub mod consts;
pub mod crypto;
pub mod file;
pub mod funder;
pub mod index_client;
pub mod index_server;
pub mod keepalive;
pub mod net;
pub mod proto_ser;
pub mod relay;
// pub mod report;
pub mod secure_channel;
pub mod ser_string;
pub mod wrapper;

// include_schema!(report_capnp, "report_capnp");
include_schema!(app_server_capnp, "app_server_capnp");
include_schema!(common_capnp, "common_capnp");
include_schema!(dh_capnp, "dh_capnp");
include_schema!(relay_capnp, "relay_capnp");
include_schema!(funder_capnp, "funder_capnp");
include_schema!(keepalive_capnp, "keepalive_capnp");
include_schema!(index_capnp, "index_capnp");
