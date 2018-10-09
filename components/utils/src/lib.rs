#![feature(generators)]
#![feature(nll)]
#![feature(try_from)]
#![crate_type = "lib"] 

extern crate byteorder;
extern crate bytes;
extern crate capnp;
#[macro_use]
// extern crate futures;
extern crate futures_await as futures;
extern crate futures_cpupool;
#[macro_use]
extern crate log;
// extern crate rusqlite;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_codec;
extern crate async_mutex;

extern crate serde;
extern crate serde_json;
extern crate base64;

mod close_handle; 
mod service_client;
mod stream_mediator;

pub mod int_convert;
pub mod safe_arithmetic;
#[macro_use]
pub mod big_array;
pub mod async_adapter;
pub mod frame_codec;
pub mod async_test_utils;

pub use self::close_handle::CloseHandle;
pub use self::service_client::{ServiceClient, ServiceClientError};
pub use self::stream_mediator::{StreamMediator, StreamMediatorError};
// pub use self::macros::TryFromBytesError;

