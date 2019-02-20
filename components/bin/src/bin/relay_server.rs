#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![type_length_limit="4194304"]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#[macro_use]
extern crate log;

use std::time::Duration;
use std::net::SocketAddr;
use std::path::Path;

use log::Level;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use clap::{Arg, App};

use common::conn::Listener;

use crypto::crypto_rand::system_random;
use identity::{create_identity, IdentityClient};

use proto::consts::{TICK_MS, MAX_FRAME_LENGTH};

use common::int_convert::usize_to_u64;

use timer::create_timer;
use relay::{net_relay_server, NetRelayServerError};
use net::TcpListener;

use proto::file::identity::load_identity_from_file;

// TODO; Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;


#[derive(Debug)]
enum RelayServerBinError {
    ParseListenAddressError,
    CreateThreadPoolError,
    LoadIdentityError,
    CreateIdentityError,
    CreateTimerError,
    NetRelayServerError(NetRelayServerError),
}

fn run() -> Result<(), RelayServerBinError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Relay Server")
                          .version("0.1.0")
                          .author("real <real@freedomlayer.org>")
                          .about("Spawns an Offst Relay Server")
                          .arg(Arg::with_name("idfile")
                               .short("i")
                               .long("idfile")
                               .value_name("idfile")
                               .help("Identity file path")
                               .required(true))
                          .arg(Arg::with_name("laddr")
                               .short("l")
                               .long("laddr")
                               .value_name("laddr")
                               .help("Listening address. \nExamples:\n- 0.0.0.0:1337\n- fe80::14c2:3048:b1ac:85fb:1337")
                               .required(true))
                          .get_matches();
    
    // Parse listening address
    let listen_address_str = matches.value_of("laddr").unwrap();
    let listen_socket_addr: SocketAddr = listen_address_str.parse()
        .map_err(|_| RelayServerBinError::ParseListenAddressError)?;

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(Path::new(&idfile_path))
        .map_err(|_| RelayServerBinError::LoadIdentityError)?;

    // Create a ThreadPool:
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| RelayServerBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool.spawn(identity_loop)
        .map_err(|_| RelayServerBinError::CreateIdentityError)?;
    let identity_client = IdentityClient::new(sender);
    

    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = create_timer(dur, thread_pool.clone()) 
        .map_err(|_| RelayServerBinError::CreateTimerError)?;

    let rng = system_random();

    let tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_raw_conns) = tcp_listener.listen(listen_socket_addr);

    let relay_server_fut = net_relay_server(incoming_raw_conns,
                             identity_client,
                             timer_client,
                             rng,
                             MAX_CONCURRENT_ENCRYPT,
                             thread_pool.clone());

    thread_pool.run(relay_server_fut)
        .map_err(|e| RelayServerBinError::NetRelayServerError(e))
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
