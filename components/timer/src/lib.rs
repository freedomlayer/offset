#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(never_type)]

#[macro_use]
extern crate log;

mod timer;
pub mod utils;

pub use self::timer::{TimerTick, TimerClient, 
    create_timer_incoming, create_timer, dummy_timer_multi_sender};
