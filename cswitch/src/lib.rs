#![feature(conservative_impl_trait)]

#[macro_use]
extern crate log;

mod uid;
mod identity;
mod inner_messages;
mod networker_state_machine;
mod rand_values;
mod prefix_frame_codec;

mod close_handle;
mod security_module;
mod channeler;

mod symmetric_enc;

// mod static_dh_hack;
mod test_utils;
mod dh;


