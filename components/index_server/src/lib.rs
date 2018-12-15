#![crate_type = "lib"] 
#![feature(futures_api, pin, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(dbg_macro)]


#[macro_use]
extern crate log;

mod option_iterator;
mod server;
mod bfs;
mod capacity_graph;
mod simple_capacity_graph;
mod graph_service;
