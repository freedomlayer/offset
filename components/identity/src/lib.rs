#![crate_type = "lib"] 
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

extern crate futures;
extern crate tokio_core;
extern crate ring;

extern crate cswitch_crypto as crypto;

mod identity;
mod client;
mod messages;


pub use client::{IdentityClient};
pub use identity::{create_identity};
