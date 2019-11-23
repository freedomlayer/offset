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

#[allow(unused)]
#[cfg(test)]
mod sim_network;

#[allow(unused)]
#[cfg(test)]
mod utils;

// #[cfg(test)]
// mod tests;

// #[cfg(test)]
// mod cli_tests;
