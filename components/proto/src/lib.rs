#![feature(try_from)]
#![feature(nll)]

extern crate capnp;
extern crate byteorder;

#[macro_use]
extern crate common;
extern crate crypto;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate bytes;

extern crate im;

#[macro_use]
pub mod macros;
pub mod consts;
pub mod capnp_common;
pub mod relay;
pub mod secure_channel;
pub mod funder;
pub mod keepalive;
pub mod serialize;
pub mod app_server;
pub mod index_server;
pub mod index_client;
pub mod report;
pub mod net;



include_schema!(report_capnp, "report_capnp");
include_schema!(app_server_capnp, "app_server_capnp");
include_schema!(common_capnp, "common_capnp");
include_schema!(dh_capnp, "dh_capnp");
include_schema!(relay_capnp, "relay_capnp");
include_schema!(funder_capnp, "funder_capnp");
include_schema!(keepalive_capnp, "keepalive_capnp");
include_schema!(index_capnp, "index_capnp");
