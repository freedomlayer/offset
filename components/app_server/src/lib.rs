#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate common;

mod server;

#[cfg(test)]
mod tests;

pub use self::server::{app_server_loop, AppServerError, IncomingAppConnection};
