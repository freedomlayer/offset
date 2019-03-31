#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate common;

mod timer;
pub mod utils;

pub use self::timer::{
    create_timer, create_timer_incoming, dummy_timer_multi_sender, TimerClient, TimerTick,
};
