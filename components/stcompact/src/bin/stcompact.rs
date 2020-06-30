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

use futures::executor::{block_on, ThreadPool};

use stcompact::stcompactlib::{stcompact, StCompactCmd, StCompactError};

fn run() -> Result<(), StCompactError> {
    env_logger::init();
    let st_compact_cmd = StCompactCmd::from_args();
    let thread_pool = ThreadPool::new().map_err(|_| StCompactError::SpawnError)?;
    let file_thread_pool = ThreadPool::new().map_err(|_| StCompactError::SpawnError)?;
    block_on(stcompact(st_compact_cmd, thread_pool, file_thread_pool))
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
