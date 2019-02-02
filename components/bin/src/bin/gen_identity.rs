#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;
extern crate simple_logger;
extern crate clap;

use std::io::{self, Write};
use std::path::PathBuf;
use std::fs::File;

use clap::{Arg, App};
use log::Level;

use crypto::crypto_rand::system_random;
use crypto::identity::generate_pkcs8_key_pair;

fn save_identity_to_file(pkcs8_buf: [u8; 85], path_buf: PathBuf) -> Result<(),io::Error> {
    let mut file = File::create(path_buf)?;
    file.write(&pkcs8_buf)?;
    Ok(())
}

fn main() {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Identity Generator")
                          .version("0.1")
                          .author("real <real@freedomlayer.org>")
                          .about("Randomly generates a new identity file")
                          .arg(Arg::with_name("output")
                               .short("o")
                               .long("output")
                               .value_name("output")
                               .help("output identity file path")
                               .required(true))
                          .get_matches();

    // Generate a new random keypair:
    let rng = system_random();
    let pkcs8 = generate_pkcs8_key_pair(&rng);

    let output_path = matches.value_of("output").unwrap();
    if let Err(e) = save_identity_to_file(pkcs8, output_path.into()) {
        error!("Failed to save generated identity to file: {:?}", e);
    }
}
