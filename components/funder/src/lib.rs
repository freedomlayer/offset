#![crate_type = "lib"]
#![feature(async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
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
extern crate serde_derive;

// #[allow(unused)]
// mod credit_calc;
#[allow(unused)]
mod ephemeral;
#[allow(unused)]
mod friend;
// mod funder;
// mod handler;
#[allow(unused)]
mod liveness;
#[allow(unused)]
mod mutual_credit;
// pub mod report;
// mod state;
// #[cfg(test)]
// mod tests;
#[allow(unused)]
mod token_channel;
pub mod types;

// pub use self::funder::{funder_loop, FunderError};
// pub use self::state::{FunderMutation, FunderState};
