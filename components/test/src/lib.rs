#![crate_type = "lib"] 
#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![feature(unboxed_closures)]
#![type_length_limit="4194304"]

#![deny(
    trivial_numeric_casts,
    warnings
)]


#[cfg(test)]
#[macro_use]
extern crate log;

#[cfg(test)]
mod sim_network;

#[cfg(test)]
mod utils;

#[cfg(test)]
mod tests;
