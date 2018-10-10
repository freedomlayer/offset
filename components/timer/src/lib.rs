#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(never_type)]

extern crate futures;

pub mod timer;

pub use self::timer::{TimerTick, TimerClient, create_timer_incoming};
