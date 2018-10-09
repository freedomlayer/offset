#![crate_type = "lib"] 
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate futures_await as futures;
extern crate tokio_core;

pub mod timer;

pub use timer::{TimerTick, TimerClient, create_timer_incoming};
