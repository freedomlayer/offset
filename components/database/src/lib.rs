#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

extern crate log;

#[cfg(test)]
#[macro_use]
extern crate serde_derive;

pub mod file_db;
mod atomic_db;
mod database;

pub use self::atomic_db::AtomicDb;
pub use self::database::{
    DatabaseClient, 
    DatabaseRequest,
    DatabaseClientError, 
    database_loop};

                    
