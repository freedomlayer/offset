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

use structopt::StructOpt;

use bin::strelay::{strelay, RelayServerBinError, StRelayCmd};

fn run() -> Result<(), RelayServerBinError> {
    env_logger::init();
    let st_relay_cmd = StRelayCmd::from_args();
    strelay(st_relay_cmd)
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
