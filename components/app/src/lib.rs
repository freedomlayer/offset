#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

// #[macro_use]
// extern crate log;
extern crate clap;
#[macro_use]
extern crate serde_derive;

mod connector;
