#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]


#[macro_use]
extern crate log;

use std::collections::HashMap;

use std::path::Path;
use std::time::Duration;
use std::net::SocketAddr;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;
use common::conn::Listener;

use crypto::crypto_rand::system_random;

use identity::{create_identity, IdentityClient};

use index_server::{net_index_server, NetIndexServerError};
use timer::create_timer;
use proto::consts::{TICK_MS, MAX_FRAME_LENGTH};

use net::{NetConnector, TcpListener};

use proto::file::identity::load_identity_from_file;
use proto::file::index_server::load_trusted_servers;

// TODO; Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;
/// Amount of ticks we wait before attempting to reconnect to a remote index server.
pub const BACKOFF_TICKS: usize = 0x8;


#[derive(Debug)]
enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    CreateNetConnectorError,
    NetIndexServerError(NetIndexServerError),
    ParseClientListenAddressError,
    ParseServerListenAddressError,
    LoadIdentityError,
    CreateIdentityError,
    LoadTrustedServersError,
}


fn run() -> Result<(), IndexServerBinError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Index Server")
                          .version("0.1.0")
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
                          .arg(Arg::with_name("trusted")
                               .short("t")
                               .long("trusted")
                               .value_name("trusted")
                               .help("Directory path of trusted index servers")
                               .required(true))
                          .get_matches();

    // Parse clients listening address
    let client_listen_address_str = matches.value_of("lclient").unwrap();
    let client_listen_socket_addr: SocketAddr = client_listen_address_str.parse()
        .map_err(|_| IndexServerBinError::ParseClientListenAddressError)?;

    // Parse servers listening address
    let server_listen_address_str = matches.value_of("lserver").unwrap();
    let server_listen_socket_addr: SocketAddr = server_listen_address_str.parse()
        .map_err(|_| IndexServerBinError::ParseServerListenAddressError)?;

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(Path::new(&idfile_path))
        .map_err(|_| IndexServerBinError::LoadIdentityError)?;
    // let local_public_key = identity.get_public_key();

    // Load trusted index servers
    let trusted_dir_path = matches.value_of("trusted").unwrap();
    // TODO: We need a more detailed error here.
    // It might be hard for the user to detect in which file there was a problem
    // in case of an error.
    let trusted_servers = load_trusted_servers(Path::new(&trusted_dir_path))
        .map_err(|_| IndexServerBinError::LoadTrustedServersError)?
        .into_iter()
        .map(|index_server_address| (index_server_address.public_key, index_server_address.address))
        .collect::<HashMap<_,_>>();

    // Create a ThreadPool:
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| IndexServerBinError::CreateThreadPoolError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool.spawn(identity_loop)
        .map_err(|_| IndexServerBinError::CreateIdentityError)?;
    let identity_client = IdentityClient::new(sender);

    // Get a timer client:
    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| IndexServerBinError::CreateTimerError)?;


    // Start listening to clients:
    let client_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_client_raw_conns) = client_tcp_listener.listen(client_listen_socket_addr);

    // Start listening to servers:
    let server_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_server_raw_conns) = server_tcp_listener.listen(server_listen_socket_addr);

    // A tcp connector, Used to connect to remote servers:
    let raw_server_net_connector = NetConnector::new(MAX_FRAME_LENGTH, thread_pool.clone())
        .map_err(|_| IndexServerBinError::CreateNetConnectorError)?;


    let rng = system_random();

    let index_server_fut = net_index_server(incoming_client_raw_conns,
                    incoming_server_raw_conns,
                    raw_server_net_connector,
                    identity_client,
                    timer_client,
                    rng,
                    trusted_servers,
                    MAX_CONCURRENT_ENCRYPT,
                    BACKOFF_TICKS,
                    thread_pool.clone());

    thread_pool.run(index_server_fut)
        .map_err(|e| IndexServerBinError::NetIndexServerError(e))?;


    Ok(())
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
