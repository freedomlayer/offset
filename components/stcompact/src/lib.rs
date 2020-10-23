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

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[macro_use]
extern crate quickcheck_derive;

pub mod compact_node;

mod gen;
pub mod messages;
pub mod server_loop;
pub mod store;

mod serialize;
pub mod stcompactlib;

// TODO: Possibly remove later?
pub use gen::GenCryptoRandom;
