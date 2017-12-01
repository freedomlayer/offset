#![feature(conservative_impl_trait)]
// TODO: Most of the current warnings cause by the following reason, will be
// removed as soon as whole project ready to work and fix those warnings.
#![allow(dead_code, unused)]

#[macro_use]
extern crate log;
extern crate futures;

mod crypto;

mod inner_messages;
mod networker_state_machine;
// mod prefix_frame_codec;

mod close_handle;
mod security_module;
mod channeler;

mod async_mutex;
mod service_client;
mod schema;
pub mod timer;
