#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

// #[macro_use]
// extern crate log;
extern crate clap;
#[macro_use]
extern crate serde_derive;

mod identity_file;
mod index_server_file;
mod relay_file;
mod app_file;

pub use self::identity_file::{load_identity_from_file, store_identity_to_file};
pub use self::index_server_file::{load_trusted_servers, store_index_server_to_file};
pub use self::relay_file::{store_relay_to_file};
pub use self::app_file::{TrustedApp, store_trusted_app_to_file, load_trusted_apps};

