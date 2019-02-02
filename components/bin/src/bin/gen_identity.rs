#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use clap::{Arg, App};
use log::Level;

use crypto::crypto_rand::system_random;
use crypto::identity::generate_pkcs8_key_pair;

use bin::store_identity_to_file;

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
    if let Err(e) = store_identity_to_file(pkcs8, output_path.into()) {
        error!("Failed to store generated identity to file: {:?}", e);
    }
}
