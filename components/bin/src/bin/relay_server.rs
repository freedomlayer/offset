#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;
extern crate clap;

use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs::File;
use std::io::Read;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use futures::channel::mpsc;

use clap::{Arg, App};

use common::conn::Listener;

use crypto::identity::{SoftwareEd25519Identity, Identity};
use crypto::crypto_rand::system_random;
use identity::{create_identity, IdentityClient};

use proto::consts::{TICK_MS, KEEPALIVE_TICKS, 
    CONN_TIMEOUT_TICKS, TICKS_TO_REKEY, MAX_FRAME_LENGTH,
    PROTOCOL_VERSION};
use proto::funder::messages::{TcpAddress, TcpAddressV4};

use common::int_convert::usize_to_u64;
use timer::create_timer;
use relay::{relay_server, RelayServerError};
use secure_channel::SecureChannel;
use version::VersionPrefix;
use net::TcpListener;

/// Load an identity from a file
/// The file stores the private key according to PKCS#8.
/// TODO: Be able to read base64 style PKCS#8 files.
fn load_identity_from_file(path_buf: PathBuf) -> Option<impl Identity> {
    let mut file = File::open(path_buf).ok()?;
    let mut buf = [0u8; 85]; // TODO: Make this more generic?
    file.read(&mut buf).ok()?;
    SoftwareEd25519Identity::from_pkcs8(&buf).ok()
}

fn main() {
    let matches = App::new("Offst Relay Server")
                          .version("0.1")
                          .author("real <real@freedomlayer.org>")
                          .about("Spawns an Offst Relay Server")
                          .arg(Arg::with_name("pkfile")
                               .short("p")
                               .long("pkfile")
                               .value_name("pkfile")
                               .help("Sets private key input file to use")
                               .required(true))
                          .get_matches();

    // Parse file an get identity:
    let pkfile_path = matches.value_of("pkfile").unwrap();
    let identity = match load_identity_from_file(pkfile_path.into()) {
        Some(identity) => identity,
        None => {
            error!("Failed to parse key file! Aborting.");
            return;
        },
    };

    // Create a ThreadPool:
    let mut thread_pool = match ThreadPool::new() {
        Ok(thread_pool) => thread_pool,
        Err(_) => {
            error!("Could not create a ThreadPool! Aborting.");
            return;
        },
    };

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    thread_pool.spawn(identity_loop);
    let identity_client = IdentityClient::new(sender);
    

    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = match create_timer(dur, thread_pool.clone()) {
        Ok(timer_client) => timer_client,
        Err(_) => {
            error!("Failed to create timer! Aborting.");
            return;
        }
    };

    let version_transform = VersionPrefix::new(PROTOCOL_VERSION,
                                               thread_pool.clone());

    let rng = system_random();
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng,
        timer_client.clone(),
        TICKS_TO_REKEY,
        thread_pool.clone());


    let tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let tcp_address = TcpAddress::V4(TcpAddressV4 {
        address: [0, 0, 0, 0], // Should be 127.0.0.1?
        port: 0, // TODO: Should be given as argument
    });

    let (_config_sender, incoming_raw_conns) = tcp_listener.listen(tcp_address);

    // TODO: 
    // - Add version prefix to every incoming connection
    // - Move transform_pool_loop() to common crate. 
    // - Use transform_pool_loop() to encrypt all incoming connections

    // let (incoming_conns_sender, incoming_conns) = mpsc::channel(0);
    thread_pool.spawn(async move {
        /*
        while let Some(raw_conn) = await!(incoming_raw_conns.next()) {
            version_transform.transform(raw_conn)
        }
        */
    });


    /*
    relay_server(incoming_conns,
                timer_client,
                CONN_TIMEOUT_TICKS,
                KEEPALIVE_TICKS,
                thread_pool.clone());
    */


    println!("Hello world!");
}
