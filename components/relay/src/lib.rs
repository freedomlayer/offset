#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![type_length_limit="2097152"]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#[macro_use]
extern crate log;

mod server;
mod client;

pub use self::client::client_listener::ClientListener;
pub use self::client::client_connector::ClientConnector;
pub use self::server::net_server::{net_relay_server, NetRelayServerError};

