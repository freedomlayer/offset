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

use structopt::StructOpt;

use bin::stindex::{stindex, IndexServerBinError, StIndexCmd};

fn run() -> Result<(), IndexServerBinError> {
    env_logger::init();

    let st_index_cmd = StIndexCmd::from_args();
    stindex(st_index_cmd)
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
