#![feature(async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]

#[macro_use]
extern crate log;

use std::io;
use structopt::StructOpt;

use stctrl::stregisterlib::{stregister, StRegisterCmd, StRegisterError};

fn run() -> Result<(), StRegisterError> {
    env_logger::init();
    let st_register_cmd = StRegisterCmd::from_args();
    stregister(st_register_cmd, &mut io::stdout())
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
