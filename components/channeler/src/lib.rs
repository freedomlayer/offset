#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate common;

mod channeler;
mod types;
mod listen_pool_state;
mod listen_pool;
mod connect_pool;
mod connector_utils;
mod overwrite_channel;
mod spawn;



pub use self::channeler::ChannelerError;
pub use self::spawn::{spawn_channeler, SpawnChannelerError};
