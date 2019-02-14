#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
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
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{FutureExt, TryFutureExt, StreamExt, SinkExt};

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;
use common::conn::{Listener, FutTransform, ConnPairVec, ConnPair, 
    BoxFuture, FuncFutTransform};
use common::transform_pool::transform_pool_loop;

use crypto::crypto_rand::system_random;
use crypto::identity::{Identity, PublicKey};

use identity::{create_identity, IdentityClient};

use index_server::{index_server, IndexServerError};
use timer::create_timer;
use proto::consts::{TICK_MS, INDEX_NODE_TIMEOUT_TICKS, 
    MAX_FRAME_LENGTH, PROTOCOL_VERSION,
    TICKS_TO_REKEY, KEEPALIVE_TICKS};

use proto::index_server::serialize::{deserialize_index_client_to_server,
                                     serialize_index_server_to_server,
                                     deserialize_index_server_to_server,
                                     serialize_index_server_to_client};
use proto::index_server::messages::{IndexClientToServer, IndexServerToClient,
                                    IndexServerToServer};

use net::{NetConnector, TcpListener};

use version::VersionPrefix;
use secure_channel::SecureChannel;
use keepalive::KeepAliveChannel;

use bin::{load_identity_from_file, load_trusted_servers};

// TODO; Maybe take as a command line argument in the future?
/// Maximum amount of concurrent encrypted channel set-ups.
/// We set this number to avoid DoS from half finished encrypted channel negotiations.
pub const MAX_CONCURRENT_ENCRYPT: usize = 0x200;
/// Amount of ticks we wait before attempting to reconnect to a remote index server.
pub const BACKOFF_TICKS: usize = 0x8;


#[derive(Clone)]
struct ConnTransformer<VT,ET,KT,S> {
    version_transform: VT,
    encrypt_transform: ET,
    keepalive_transform: KT,
    spawner: S,
}

impl<VT,ET,KT,S> ConnTransformer<VT,ET,KT,S> 
where
    VT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>> + Clone + Send,
    KT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Clone + Send,
    S: Spawn + Clone + Send,
{
    pub fn new(version_transform: VT,
               encrypt_transform: ET,
               keepalive_transform: KT,
               spawner: S) -> Self {

        ConnTransformer {
            version_transform,
            encrypt_transform,
            keepalive_transform,
            spawner,
        }
    }

    fn version_enc_keepalive(&self, 
                             opt_public_key: Option<PublicKey>, 
                             conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPairVec)>> 
    {
        let mut c_version_transform = self.version_transform.clone();
        let mut c_encrypt_transform = self.encrypt_transform.clone();
        let mut c_keepalive_transform = self.keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = await!(c_version_transform.transform(conn_pair));
            let (public_key, conn_pair) =
                await!(c_encrypt_transform.transform((opt_public_key, conn_pair)))?;
            let conn_pair = await!(c_keepalive_transform.transform(conn_pair));
            Some((public_key, conn_pair))
        })
    }


    /// Transform a raw connection from a client into connection with the following layers:
    /// - Version prefix
    /// - Encryption
    /// - keepalives
    /// - Serialization
    pub fn incoming_index_client_conn_transform(&self, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPair<IndexServerToClient, IndexClientToServer>)>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(None, conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_client_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_client(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    }

    pub fn incoming_index_server_conn_transform(&self, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<(PublicKey, ConnPair<IndexServerToServer, IndexServerToServer>)>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(None, conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_server_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_server(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    }

    pub fn outgoing_index_server_conn_transform(&self, public_key: PublicKey, conn_pair: ConnPairVec)
                    -> BoxFuture<'_, Option<ConnPair<IndexServerToServer, IndexServerToServer>>> 
    {
        let mut c_self = self.clone();
        Box::pin(async move {
            let (_public_key, (mut sender, mut receiver)) = await!(c_self.version_enc_keepalive(Some(public_key), conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            let _ = c_self.spawner.spawn(async move {
                while let Some(data) = await!(receiver.next()) {
                    let message = match deserialize_index_server_to_server(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(to_user_receiver.send(message)) {
                        return;
                    }
                }
            });

            // Serialize sent data:
            let _ = c_self.spawner.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_server(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((user_sender, user_receiver))
        })
    }
}

#[derive(Debug)]
enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    CreateNetConnectorError,
    IndexServerError(IndexServerError),
    ParseClientListenAddressError,
    ParseServerListenAddressError,
    LoadIdentityError,
    CreateIdentityError,
    SpawnError,
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
    let local_public_key = identity.get_public_key();

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


    let version_transform = VersionPrefix::new(PROTOCOL_VERSION,
                                               thread_pool.clone());
    let rng = system_random();
    let encrypt_transform = SecureChannel::new(
        identity_client,
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        thread_pool.clone());

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        KEEPALIVE_TICKS,
        thread_pool.clone());

    let conn_transformer = ConnTransformer::new(version_transform,
                         encrypt_transform,
                         keepalive_transform,
                         thread_pool.clone());

    // Transform incoming client connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_client_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            await!(c_conn_transformer.incoming_index_client_conn_transform(raw_conn))
        })
    });
    let (client_conns_sender, incoming_client_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(incoming_client_raw_conns,
                        client_conns_sender,
                        incoming_client_transform,
                        MAX_CONCURRENT_ENCRYPT,
                        thread_pool.clone())
        .map_err(|e| error!("client incoming transform_pool_loop() error: {:?}", e))
        .map(|_| ());
    thread_pool.spawn(pool_fut)
        .map_err(|_| IndexServerBinError::SpawnError)?;

    // Transform incoming server connections:
    let c_conn_transformer = conn_transformer.clone();
    let incoming_server_transform = FuncFutTransform::new(move |raw_conn| {
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            await!(c_conn_transformer.incoming_index_server_conn_transform(raw_conn))
        })
    });
    let (server_conns_sender, incoming_server_conns) = mpsc::channel(0);
    let pool_fut = transform_pool_loop(incoming_server_raw_conns,
                        server_conns_sender,
                        incoming_server_transform,
                        MAX_CONCURRENT_ENCRYPT,
                        thread_pool.clone())
        .map_err(|e| error!("server incoming transform_pool_loop() error: {:?}", e))
        .map(|_| ());
    thread_pool.spawn(pool_fut)
        .map_err(|_| IndexServerBinError::SpawnError)?;

    // Apply transform to create server connector:
    let c_conn_transformer = conn_transformer.clone();
    let server_connector = FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_raw_server_net_connector = raw_server_net_connector.clone();
        let c_conn_transformer = c_conn_transformer.clone();
        Box::pin(async move {
            let raw_conn = await!(c_raw_server_net_connector.transform(net_address))?;
            await!(c_conn_transformer.outgoing_index_server_conn_transform(public_key, raw_conn))
        })
    });

    let index_server_fut = index_server(local_public_key,
                   trusted_servers,
                   incoming_server_conns,
                   incoming_client_conns,
                   server_connector,
                   timer_client,
                   INDEX_NODE_TIMEOUT_TICKS,
                   BACKOFF_TICKS,
                   rng,
                   thread_pool.clone());

    thread_pool.run(index_server_fut)
        .map_err(|e| IndexServerBinError::IndexServerError(e))?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
