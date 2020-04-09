#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

extern crate futures;

mod client;
mod identity;
mod messages;

pub use crate::client::{IdentityClient, IdentityClientError};
pub use crate::identity::create_identity;
