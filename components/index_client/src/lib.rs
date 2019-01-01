#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(dbg_macro)]


#[macro_use]
extern crate log;

mod single_client;
mod index_client;
mod client_session;
mod seq_map;
mod seq_friends;

#[cfg(test)]
mod tests;
