#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
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

use stctrl::stverifylib::{stverify, StVerifyCmd, StVerifyError};

fn run() -> Result<(), StVerifyError> {
    env_logger::init();
    let st_verify_cmd = StVerifyCmd::from_args();
    stverify(st_verify_cmd, &mut io::stdout())
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
