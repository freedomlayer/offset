#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(generators)]
#![feature(nll)]
#![feature(try_from)]
#![crate_type = "lib"] 

#![deny(
    trivial_numeric_casts,
    warnings
)]

#[macro_use]
extern crate log;

pub mod int_convert;
pub mod safe_arithmetic;
#[macro_use]
pub mod big_array;
#[macro_use]
pub mod define_fixed_bytes;
pub mod async_adapter;
// pub mod frame_codec;
pub mod async_test_utils;
pub mod futures_compat;
pub mod conn;
pub mod dummy_connector;
pub mod dummy_listener;
pub mod access_control;
pub mod canonical_serialize;
pub mod mutable_state;
pub mod transform_pool;
pub mod state_service;
pub mod multi_consumer;
pub mod select_streams;
