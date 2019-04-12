#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

mod net_connector;
mod resolver;
mod tcp_connector;
mod tcp_listener;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use self::net_connector::NetConnector;
pub use self::tcp_listener::TcpListener;
