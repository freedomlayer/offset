#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]
// TODO; Remove this hint:
#![allow(unused)]

#[macro_use]
extern crate prettytable;

#[macro_use]
extern crate quickcheck_derive;

// #[macro_use]
// extern crate log;

#[macro_use]
extern crate serde;

// mod multi_route_util;

pub mod buyer;
pub mod config;
pub mod file;
pub mod info;
pub mod seller;
pub mod utils;

pub mod stctrllib;
pub mod stverifylib;
