#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(dbg_macro)]
#![type_length_limit="2097152"]

#[macro_use]
extern crate log;

#[allow(unused)]
mod server;

