#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::time::Duration;

use futures::executor::ThreadPool;

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;

use crypto::crypto_rand::system_random;
use index_server::{index_server, IndexServerError};
use timer::create_timer;
use proto::consts::{TICK_MS, INDEX_NODE_TIMEOUT_TICKS};


#[derive(Debug)]
enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    RequestTimerStreamError,
    IndexServerError(IndexServerError),
}

fn run() -> Result<(), IndexServerBinError> {
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
                          .arg(Arg::with_name("lclient")
                               .long("lclient")
                               .value_name("lclient")
                               .help("Listening address for clients")
                               .required(true))
                          .arg(Arg::with_name("lserver")
                               .long("lserver")
                               .value_name("lserver")
                               .help("Listening address for servers")
                               .required(true))
                          .arg(Arg::with_name("config")
                               .short("c")
                               .long("config")
                               .value_name("config")
                               .help("Configuration file path")
                               .required(true))
                          .get_matches();

    // Create a ThreadPool:
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let mut timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| IndexServerBinError::CreateTimerError)?;

    // Get one timer_stream:
    let timer_stream = thread_pool.run(timer_client.request_timer_stream())
        .map_err(|_| IndexServerBinError::RequestTimerStreamError)?;

    /*
    let rng = system_random();
    let index_server_fut = index_server(index_server_config: IndexServerConfig<A>,
                   incoming_server_connections: IS,
                   incoming_client_connections: IC,
                   server_connector: SC,
                   timer_stream,
                   INDEX_NODE_TIMEOUT_TICKS,
                   rng,
                   thread_pool);

    thread_pool.run(index_server_fut)
        .map_err(|e| IndexServerBinError::IndexServerError(e))
   */
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
