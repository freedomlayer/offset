#![crate_type = "lib"]
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]

#[macro_use]
extern crate log;

mod version_prefix;

pub use self::version_prefix::VersionPrefix;
