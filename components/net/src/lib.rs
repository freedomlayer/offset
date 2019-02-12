#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

// #[macro_use]
extern crate log;

mod types;
mod utils;
mod resolver;
mod tcp_connector;
mod tcp_listener;
mod net_connector;
#[cfg(test)]
mod tests;


pub use self::tcp_connector::TcpConnector;
pub use self::tcp_listener::TcpListener;
