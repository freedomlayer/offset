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

use bin::stmgrlib::{stmgr, StMgrCmd, StmError};

fn run() -> Result<(), StmError> {
    env_logger::init();

    let st_mgr_cmd = StMgrCmd::from_args();
    stmgr(st_mgr_cmd)
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
