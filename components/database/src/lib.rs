#![crate_type = "lib"]
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
#[allow(unused)]
mod create;
mod database;
pub mod file_db;

pub use self::atomic_db::AtomicDb;
pub use self::database::{database_loop, DatabaseClient, DatabaseClientError, DatabaseRequest};
