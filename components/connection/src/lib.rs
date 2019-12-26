#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

extern crate log;

mod transforms;

pub use self::transforms::{
    create_encrypt_keepalive, create_secure_connector, create_version_encrypt_keepalive,
};
