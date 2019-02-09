#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

mod server;

#[cfg(test)]
mod tests;

pub use self::server::{app_server_loop, 
    IncomingAppConnection};
