#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::path::{Path, PathBuf};

use clap::{Arg, App};
use log::Level;

use crypto::crypto_rand::system_random;
use crypto::identity::{generate_pkcs8_key_pair, Identity};

use proto::funder::messages::RelayAddress;
use proto::index_server::messages::IndexServerAddress;

use database::file_db::FileDb;
use node::NodeState;

use bin::load_identity_from_file;

#[derive(Debug)]
enum InitNodeDbBinError {
    OutputAlreadyExists,
    LoadIdentityError,
    FileDbError,
}

fn run() -> Result<(), InitNodeDbBinError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Node database initializer")
                          .version("0.1")
                          .author("real <real@freedomlayer.org>")
                          .about("Initializes a new empty node database")
                          .arg(Arg::with_name("output")
                               .short("o")
                               .long("output")
                               .value_name("output")
                               .help("output database file path")
                               .required(true))
                          .arg(Arg::with_name("idfile")
                               .short("i")
                               .long("idfile")
                               .value_name("idfile")
                               .help("identity file path")
                               .required(true))
                          .get_matches();

    // Make sure that output_path does not exist.
    // This program should never override any file! (Otherwise users might erase their database by
    // accident).
    let output_path = PathBuf::from(&matches.value_of("output").unwrap());
    if output_path.exists() {
        return Err(InitNodeDbBinError::OutputAlreadyExists);
    }

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(Path::new(&idfile_path))
        .map_err(|_| InitNodeDbBinError::LoadIdentityError)?;
    let local_public_key = identity.get_public_key();

    // Create a new database file:
    let initial_state = NodeState::<RelayAddress, IndexServerAddress>::new(local_public_key);
    let file_db = FileDb::create(output_path.to_path_buf(), initial_state)
        .map_err(|_| InitNodeDbBinError::FileDbError)?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}

