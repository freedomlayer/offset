#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

mod timeout;
mod transforms;

pub use self::transforms::{
    create_encrypt_keepalive, create_secure_connector, create_version_encrypt_keepalive,
};
