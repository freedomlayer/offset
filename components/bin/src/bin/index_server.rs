#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use clap::{Arg, App};
use log::Level;

fn main() {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Index Server")
                          .version("0.1")
                          .author("real <real@freedomlayer.org>")
                          .about("Spawns an Index Server")
                          .arg(Arg::with_name("idfile")
                               .short("i")
                               .long("idfile")
                               .value_name("idfile")
                               .help("identity file path")
                               .required(true))
                          .get_matches();

}
