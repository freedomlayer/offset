#![crate_type = "lib"]
#![cfg_attr(not(feature = "cargo-clippy"), allow(unknown_lints))]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

// #[macro_use]
// extern crate log;

#[macro_use]
extern crate serde;

#[macro_use]
extern crate quickcheck_derive;

// mod ephemeral;
// mod friend;
// mod funder;
// mod handler;
// mod liveness;
#[allow(unused)]
mod mutual_credit;
// pub mod report;
// mod state;
// mod token_channel;
pub mod types;

// #[cfg(test)]
// mod tests;

// pub use self::funder::{funder_loop, FunderError};
// pub use self::state::{FunderMutation, FunderState};
