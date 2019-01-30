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
mod spawn;

#[cfg(test)]
mod tests;

pub use self::index_client::{IndexClientError, IndexClientConfig, IndexClientConfigMutation};
pub use self::spawn::{spawn_index_client, SpawnIndexClientError};
