#![feature(nll)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

extern crate byteorder;
extern crate capnp;

extern crate common;
extern crate crypto;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate bytes;

extern crate base64;
extern crate im;
extern crate toml;

#[cfg(test)]
extern crate tempfile;

#[macro_use]
extern crate derive_more;

#[macro_use]
pub mod macros;
pub mod app_server;
pub mod capnp_common;
pub mod consts;
pub mod file;
pub mod funder;
pub mod index_client;
pub mod index_server;
pub mod keepalive;
pub mod net;
pub mod node;
pub mod relay;
pub mod report;
pub mod secure_channel;
pub mod serialize;

include_schema!(report_capnp, "report_capnp");
include_schema!(app_server_capnp, "app_server_capnp");
include_schema!(common_capnp, "common_capnp");
include_schema!(dh_capnp, "dh_capnp");
include_schema!(relay_capnp, "relay_capnp");
include_schema!(funder_capnp, "funder_capnp");
include_schema!(keepalive_capnp, "keepalive_capnp");
include_schema!(index_capnp, "index_capnp");
