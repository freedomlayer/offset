#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]

extern crate futures;

mod client;
mod identity;
mod messages;

pub use crate::client::IdentityClient;
pub use crate::identity::create_identity;
