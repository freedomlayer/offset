#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

extern crate log;

mod types;
mod utils;
mod resolver;
mod tcp_connector;
mod tcp_listener;
mod net_connector;
#[cfg(test)]
mod tests;

pub use self::net_connector::NetConnector;
pub use self::tcp_listener::TcpListener;
