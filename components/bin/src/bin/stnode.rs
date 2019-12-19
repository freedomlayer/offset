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

use bin::stnode::{stnode, NodeBinError, StNodeCmd};

fn run() -> Result<(), NodeBinError> {
    env_logger::init();
    let st_node_cmd = StNodeCmd::from_args();
    stnode(st_node_cmd)
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
