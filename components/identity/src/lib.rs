#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate futures;
// extern crate tokio_core;

extern crate cswitch_crypto as crypto;
extern crate cswitch_utils as utils;

mod identity;
mod client;
mod messages;


pub use client::{IdentityClient};
pub use identity::{create_identity};
