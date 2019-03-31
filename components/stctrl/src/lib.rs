#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]

#[macro_use]
extern crate prettytable;
#[macro_use]
extern crate log;

pub mod config;
pub mod funds;
pub mod info;
