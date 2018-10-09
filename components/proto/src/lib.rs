#![feature(try_from)]
#![feature(nll)]

extern crate capnp;
extern crate byteorder;

extern crate cswitch_crypto as crypto;

#[macro_use]
pub mod macros;
pub mod capnp_custom_int;
pub mod relay;
pub mod secure_channel;


include_schema!(channeler_capnp, "channeler_capnp");
include_schema!(common_capnp, "common_capnp");
include_schema!(dh_capnp, "dh_capnp");
include_schema!(relay_capnp, "relay_capnp");
