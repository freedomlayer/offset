#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]
#![type_length_limit="4194304"]

#[macro_use]
extern crate log;

use std::time::Duration;
use std::net::SocketAddr;

use log::Level;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use futures::channel::mpsc;
use futures::{FutureExt, TryFutureExt, StreamExt};

use clap::{Arg, App};

use common::conn::{Listener, FutTransform, ConnPairVec, BoxFuture};

use crypto::identity::PublicKey;
use crypto::crypto_rand::system_random;
use identity::{create_identity, IdentityClient};

use proto::consts::{TICK_MS, KEEPALIVE_TICKS, 
    CONN_TIMEOUT_TICKS, TICKS_TO_REKEY, MAX_FRAME_LENGTH,
    PROTOCOL_VERSION, MAX_CONCURRENT_ENCRYPT};

use common::int_convert::usize_to_u64;
use common::transform_pool::transform_pool_loop;

use timer::create_timer;
use relay::{relay_server, RelayServerError};
use secure_channel::SecureChannel;
use version::VersionPrefix;
use net::{TcpListener, socket_addr_to_tcp_address};

use bin::load_identity_from_file;


/// Start a secure channel without knowing the identity of the remote
/// side ahead of time.
#[derive(Clone)]
struct AnonSecureChannel<ET> {
    encrypt_transform: ET,
}

impl<ET> AnonSecureChannel<ET> {
    pub fn new(encrypt_transform: ET) -> Self {
        AnonSecureChannel {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for AnonSecureChannel<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>>,
{
    type Input = ConnPairVec;
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, conn_pair: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        self.encrypt_transform.transform((None, conn_pair))
    }
}

#[derive(Debug)]
enum RelayServerBinError {
    ParseListenAddressError,
    CreateThreadPoolError,
    LoadIdentityError,
    CreateIdentityError,
    CreateTimerError,
    SpawnEncryptPoolError,
    RelayServerError(RelayServerError),
}

fn run() -> Result<(), RelayServerBinError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("Offst Relay Server")
                          .version("0.1")
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
    let socket_addr: SocketAddr = listen_address_str.parse()
        .map_err(|_| RelayServerBinError::ParseListenAddressError)?;
    let listen_tcp_address = socket_addr_to_tcp_address(&socket_addr);

    // Parse file an get identity:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(idfile_path.into())
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
    let (_config_sender, incoming_raw_conns) = tcp_listener.listen(listen_tcp_address);


    // TODO; How to get rid of Box::pin() here?
    let incoming_ver_conns = Box::pin(incoming_raw_conns
        .then(move |raw_conn| {
            // TODO: A more efficient way to do this?
            // We seem to have to clone version_transform for every connection
            // to make the borrow checker happy.
            let mut c_version_transform = version_transform.clone();
            async move {
                await!(c_version_transform.transform(raw_conn))
            }
        }));

    let (enc_conns_sender, incoming_enc_conns) = mpsc::channel::<(PublicKey, ConnPairVec)>(0);

    let enc_pool_fut = transform_pool_loop(
            incoming_ver_conns,
            enc_conns_sender,
            AnonSecureChannel::new(encrypt_transform),
            MAX_CONCURRENT_ENCRYPT,
            thread_pool.clone())
        .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
        .map(|_| ());

    thread_pool.spawn(enc_pool_fut)
        .map_err(|_| RelayServerBinError::SpawnEncryptPoolError)?;

    let relay_server_fut = relay_server(incoming_enc_conns,
                timer_client,
                CONN_TIMEOUT_TICKS,
                KEEPALIVE_TICKS,
                thread_pool.clone());

    thread_pool.run(relay_server_fut)
        .map_err(|e| RelayServerBinError::RelayServerError(e))
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
