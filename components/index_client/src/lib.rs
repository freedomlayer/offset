#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]


#[macro_use]
extern crate log;

mod single_client;
mod index_client;
mod client_session;
mod seq_map;
mod seq_friends;

#[cfg(test)]
mod tests;

pub use self::index_client::{index_client_loop,
                            IndexClientConfigMutation,
                            IndexClientConfig,
                            IndexClientError};

pub use self::seq_friends::create_seq_friends_service;
pub use self::seq_map::SeqMap;
pub use self::client_session::IndexClientSession;
pub use self::single_client::ServerConn;
