#![crate_type = "lib"] 
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![type_length_limit="2097152"]

extern crate futures_await as futures;
extern crate futures_cpupool;
extern crate tokio_core;
extern crate capnp;
#[macro_use]
extern crate log;

extern crate byteorder;
extern crate ring;

extern crate cswitch_crypto as crypto;
extern crate cswitch_proto as proto;
extern crate cswitch_utils as utils;
extern crate cswitch_timer as timer;
extern crate cswitch_identity as identity;

mod messages;
mod serialize;
mod state;
pub mod secure_channel;
