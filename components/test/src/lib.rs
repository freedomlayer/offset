#![crate_type = "lib"]
#![type_length_limit = "20844883"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
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

// TODO: Remove unused here?
#[allow(unused)]
#[cfg(test)]
mod sim_network;

// TODO: Remove unused here?
#[allow(unused)]
#[cfg(test)]
mod utils;

// TODO: Remove unused here?
#[allow(unused)]
#[cfg(test)]
mod app_wrapper;

#[cfg(test)]
mod compact_node_wrapper;

#[cfg(test)]
mod node_report_service;

#[cfg(test)]
mod compact_report_service;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cli_tests;
