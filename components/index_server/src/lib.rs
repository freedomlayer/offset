#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(map_get_key_value)]


#[macro_use]
extern crate log;

mod server;
mod graph;
mod verifier;
mod backoff_connector;
mod spawn;

pub use spawn::{index_server, IndexServerError,
                ServerConn, ClientConn};
