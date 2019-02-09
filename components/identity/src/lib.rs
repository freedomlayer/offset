#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

extern crate futures;

mod identity;
mod client;
mod messages;


pub use crate::client::{IdentityClient};
pub use crate::identity::{create_identity};
