#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(dbg_macro)]
#![cfg_attr(not(feature = "cargo-clippy"), allow(unknown_lints))]

// TODO: Remove this later:

// extern crate byteorder;
// extern crate bytes;
// extern crate futures;
// extern crate futures_await as futures;
// extern crate futures;
extern crate futures_cpupool;
#[macro_use]
extern crate log;
// extern crate rusqlite;
// extern crate tokio_core;
// extern crate tokio_io;
// extern crate tokio_codec;

extern crate num_traits;
extern crate num_bigint;

extern crate serde;
#[macro_use]
extern crate serde_derive;
// extern crate serde_json;
// extern crate base64;

// extern crate atomicwrites;

// extern crate im;

#[macro_use]
extern crate utils;
// extern crate offst_crypto as crypto;
// extern crate offst_identity as identity;


mod liveness;
mod ephemeral;
mod credit_calc;
mod freeze_guard;
mod signature_buff; 
mod friend;
mod state;
mod types;
mod mutual_credit;
mod token_channel;
mod handler;
mod report;
mod database;
mod funder;
mod consts;
#[cfg(test)]
mod tests;
