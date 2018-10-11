#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(never_type)]
#![feature(dbg_macro)]

extern crate futures;
extern crate futures_util;
extern crate futures_01;
extern crate tokio_timer;
#[macro_use]
extern crate log;

pub mod timer;

pub use self::timer::{TimerTick, TimerClient, create_timer_incoming};
