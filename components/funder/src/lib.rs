#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![cfg_attr(not(feature = "cargo-clippy"), allow(unknown_lints))]

extern crate futures_cpupool;
#[macro_use]
extern crate log;

extern crate num_traits;
extern crate num_bigint;

extern crate serde;
#[macro_use]
extern crate serde_derive;

extern crate common;


mod liveness;
mod ephemeral;
mod credit_calc;
mod friend;
mod state;
mod types;
mod mutual_credit;
mod token_channel;
mod handler;
mod report;
mod funder;
#[cfg(test)]
mod tests;

pub use self::state::FunderMutation;
pub use self::funder::funder_loop;
