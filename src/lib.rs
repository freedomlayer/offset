#![crate_type = "lib"] 
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate byteorder;
extern crate bytes;
extern crate capnp;
// extern crate futures;
extern crate futures_await as futures;
extern crate futures_cpupool;
// extern crate rusqlite;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_codec;
extern crate async_mutex;

extern crate num_traits;
extern crate num_bigint;


extern crate cswitch_proto as proto;
extern crate cswitch_utils as utils;
extern crate cswitch_crypto as crypto;
extern crate cswitch_crypto as crypto;

// Modules
// pub mod app_manager;
pub mod channeler;



