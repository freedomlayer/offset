#![feature(async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

use std::io;
use structopt::StructOpt;

use stwebpay::stwebpaylib::{stwebpay, StWebPayCmd, StWebPayError};

fn run() -> Result<(), StWebPayError> {
    env_logger::init();
    let st_web_pay_cmd = StWebPayCmd::from_args();
    stwebpay(st_web_pay_cmd, &mut io::stdout())
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
