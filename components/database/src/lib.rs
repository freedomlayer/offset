#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate log;

#[cfg(test)]
#[macro_use]
extern crate serde_derive;

mod file_db;
mod atomic_db;
mod database;

pub use self::database::{
    DatabaseClient, 
    DatabaseRequest,
    DatabaseClientError, 
    database_loop};
                    
