#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate common;

mod keepalive;

pub use self::keepalive::KeepAliveChannel;


