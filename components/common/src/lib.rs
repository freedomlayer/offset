#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate serde;

pub mod int_convert;
pub mod never;
pub mod safe_arithmetic;
#[macro_use]
pub mod big_array;
#[macro_use]
pub mod define_fixed_bytes;
// pub mod async_adapter;
// pub mod frame_codec;
pub mod access_control;
pub mod async_test_utils;
pub mod caller_info;
// pub mod canonical_serialize;
pub mod conn;
pub mod dummy_connector;
pub mod dummy_listener;
pub mod futures_compat;
pub mod multi_consumer;
pub mod mutable_state;
pub mod select_streams;
pub mod state_service;
pub mod transform_pool;
// pub mod wait_spawner;
#[macro_use]
pub mod ser_utils;
pub mod test_executor;
