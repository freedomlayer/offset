#![crate_type = "lib"]
#![type_length_limit = "2097152"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate common;

#[macro_use]
extern crate log;

mod secure_channel;
mod state;
mod types;

pub use self::secure_channel::SecureChannel;
