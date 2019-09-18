#![crate_type = "lib"]
#![feature(arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[cfg(test)]
#[macro_use]
extern crate serde;

mod atomic_db;
mod database;
pub mod file_db;

pub use self::atomic_db::AtomicDb;
pub use self::database::{database_loop, DatabaseClient, DatabaseClientError, DatabaseRequest};
