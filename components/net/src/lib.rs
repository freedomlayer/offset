#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]

// #[macro_use]
extern crate log;

mod types;
mod utils;
mod tcp_connector;
mod tcp_listener;
#[cfg(test)]
mod tests;


pub use self::tcp_connector::TcpConnector;
pub use self::tcp_listener::TcpListener;
pub use self::types::{socket_addr_to_tcp_address, tcp_address_to_socket_addr};
