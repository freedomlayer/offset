#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#![allow(unused)]

#[macro_use]
extern crate log;

pub mod identity;
pub mod connect;
