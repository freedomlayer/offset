#![crate_type = "lib"]
#![type_length_limit = "20844883"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[cfg(test)]
#[macro_use]
extern crate log;

#[cfg(test)]
#[macro_use]
extern crate common;

#[cfg(test)]
extern crate quickcheck;

// #[cfg(test)]
// #[macro_use(quickcheck)]
// extern crate quickcheck_macros;

/*
#[cfg(test)]
#[macro_use]
extern crate quickcheck_derive;
*/

#[cfg(test)]
mod sim_network;

#[cfg(test)]
mod utils;

#[cfg(test)]
mod app_wrapper;

#[cfg(test)]
mod compact_node_wrapper;

#[cfg(test)]
mod compact_server_wrapper;

#[cfg(test)]
mod node_report_service;

#[cfg(test)]
mod compact_report_service;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cli_tests;
