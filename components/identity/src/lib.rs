#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate futures;
// extern crate tokio_core;

// extern crate offst_crypto as crypto;
// extern crate offst_utils as utils;

mod identity;
mod client;
mod messages;


pub use crate::client::{IdentityClient};
pub use crate::identity::{create_identity};
