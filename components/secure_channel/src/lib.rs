#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![type_length_limit = "2097152"]
#![deny(trivial_numeric_casts, warnings)]

#[macro_use]
extern crate common;

#[macro_use]
extern crate log;

mod secure_channel;
mod state;

pub use self::secure_channel::SecureChannel;
