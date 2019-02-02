#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::time::Duration;
use std::net::SocketAddr;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use futures::channel::mpsc;
use futures::{StreamExt, SinkExt};

use clap::{Arg, App};
use log::Level;

use common::int_convert::usize_to_u64;
use common::conn::{Listener, FutTransform, ConnPairVec, BoxFuture,
                    FuncFutTransform};
use common::transform_pool::transform_pool_loop;

use crypto::crypto_rand::system_random;
use crypto::identity::PublicKey;

use identity::{create_identity, IdentityClient};

use index_server::{index_server, IndexServerError};
use timer::create_timer;
use proto::consts::{TICK_MS, INDEX_NODE_TIMEOUT_TICKS, 
    MAX_FRAME_LENGTH, PROTOCOL_VERSION,
    TICKS_TO_REKEY, KEEPALIVE_TICKS};

use proto::index_server::serialize::{serialize_index_client_to_server,
                                     deserialize_index_client_to_server,
                                     serialize_index_server_to_server,
                                     deserialize_index_server_to_server,
                                     serialize_index_server_to_client,
                                     deserialize_index_server_to_client};

use net::{TcpConnector, TcpListener, socket_addr_to_tcp_address};

use version::VersionPrefix;
use secure_channel::SecureChannel;
use keepalive::KeepAliveChannel;

use bin::load_identity_from_file;

/*
#[derive(Clone)]
struct VersionEncKeepalive<VT,ET,KT> {
    version_transform: VT,
    encrypt_transform: ET,
    keepalive_transform: KT,
}

impl<VT,ET,KT> VersionEncKeepalive<VT,ET,KT> {
    pub fn new(version_transform: VT,
               encrypt_transform: ET,
               keepalive_transform: KT) -> Self {

        VersionEncKeepalive {
            version_transform,
            encrypt_transform,
            keepalive_transform,
        }
    }
}

impl<VT,ET,KT> FutTransform for VersionEncKeepalive<VT,ET,KT>
where
    VT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),
                     Output=Option<(PublicKey, ConnPairVec)>> + Send,
    KT: FutTransform<Input=ConnPairVec, Output=ConnPairVec> + Send,
{
    type Input = ConnPairVec;
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, conn_pair: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let conn_pair = await!(self.version_transform.transform(conn_pair));
            let (public_key, conn_pair) =
                await!(self.encrypt_transform.transform((None, conn_pair)))?;
            let conn_pair = await!(self.keepalive_transform.transform(conn_pair));
            Some((public_key, conn_pair))
        })
    }
}
*/

#[derive(Debug)]
enum IndexServerBinError {
    CreateThreadPoolError,
    CreateTimerError,
    RequestTimerStreamError,
    IndexServerError(IndexServerError),
    ParseClientListenAddressError,
    ParseServerListenAddressError,
    LoadIdentityError,
    CreateIdentityError,
}

/*
    let c_thread_pool = thread_pool.clone();
    let c_version_enc_keepalive = version_enc_keepalive.clone();
    let incoming_client_transform = FuncFutTransform::new(move |conn_pair| {
        let mut c_thread_pool = c_thread_pool.clone();
        let c_version_enc_keepalive = c_version_enc_keepalive.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_version_enc_keepalive(conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            c_thread_pool.spawn(async move {
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
            c_thread_pool.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_client(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    });
*/
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

    // Parse clients listening address
    let client_listen_address_str = matches.value_of("lclient").unwrap();
    let socket_addr: SocketAddr = client_listen_address_str.parse()
        .map_err(|_| IndexServerBinError::ParseClientListenAddressError)?;
    let client_listen_tcp_address = socket_addr_to_tcp_address(&socket_addr);

    // Parse servers listening address
    let server_listen_address_str = matches.value_of("lserver").unwrap();
    let socket_addr: SocketAddr = client_listen_address_str.parse()
        .map_err(|_| IndexServerBinError::ParseServerListenAddressError)?;
    let server_listen_tcp_address = socket_addr_to_tcp_address(&socket_addr);

    // Parse identity file:
    let idfile_path = matches.value_of("idfile").unwrap();
    let identity = load_identity_from_file(idfile_path.into())
        .map_err(|_| IndexServerBinError::LoadIdentityError)?;

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
    let mut timer_client = create_timer(dur, thread_pool.clone())
        .map_err(|_| IndexServerBinError::CreateTimerError)?;

    // Get one timer_stream:
    let timer_stream = thread_pool.run(timer_client.request_timer_stream())
        .map_err(|_| IndexServerBinError::RequestTimerStreamError)?;


    // Start listening to clients:
    let client_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_client_raw_conns) = client_tcp_listener.listen(client_listen_tcp_address);

    // Start listening to servers:
    let server_tcp_listener = TcpListener::new(MAX_FRAME_LENGTH, thread_pool.clone());
    let (_config_sender, incoming_server_raw_conns) = server_tcp_listener.listen(server_listen_tcp_address);

    // A tcp connector, Used to connect to remote servers:
    let server_tcp_connector = TcpConnector::new(MAX_FRAME_LENGTH, thread_pool.clone());


    let mut version_transform = VersionPrefix::new(PROTOCOL_VERSION,
                                               thread_pool.clone());
    let rng = system_random();
    let mut encrypt_transform = SecureChannel::new(
        identity_client,
        rng.clone(),
        timer_client.clone(),
        TICKS_TO_REKEY,
        thread_pool.clone());

    let mut keepalive_transform = KeepAliveChannel::new(
        timer_client,
        KEEPALIVE_TICKS,
        thread_pool.clone());

    let version_enc_keepalive = move |conn_pair| {
        // TODO: Is there a more efficient way than cloning all the transforms here?
        let mut c_version_transform = version_transform.clone();
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_keepalive_transform = keepalive_transform.clone();
        Box::pin(async move {
            let conn_pair = await!(c_version_transform.transform(conn_pair));
            let (public_key, conn_pair) =
                await!(c_encrypt_transform.transform((None, conn_pair)))?;
            let conn_pair = await!(c_keepalive_transform.transform(conn_pair));
            Some((public_key, conn_pair))
        })
    };

    let c_thread_pool = thread_pool.clone();
    let c_version_enc_keepalive = version_enc_keepalive.clone();
    let incoming_client_transform = FuncFutTransform::new(move |conn_pair| {
        let mut c_thread_pool = c_thread_pool.clone();
        let c_version_enc_keepalive = c_version_enc_keepalive.clone();
        Box::pin(async move {
            let (public_key, (mut sender, mut receiver)) = await!(c_version_enc_keepalive(conn_pair))?;

            let (user_sender, mut from_user_sender) = mpsc::channel(0);
            let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

            // Deserialize received data
            c_thread_pool.spawn(async move {
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
            c_thread_pool.spawn(async move {
                while let Some(message) = await!(from_user_sender.next()) {
                    let data = serialize_index_server_to_client(&message);
                    if let Err(_) = await!(sender.send(data)) {
                        return;
                    }
                }
            });

            Some((public_key, (user_sender, user_receiver)))
        })
    });

    // TODO:
    // - For every connections:
    //      - Version prefix
    //      - Encrypt transform (Should be done using a pool)
    //      - keepalive transform
    //
    //      - Serialization (Different for each connection type)
    
    /*
    // IS: Stream<Item=(PublicKey, ServerConn)> + Unpin,
    // IC: Stream<Item=(PublicKey, ClientConn)> + Unpin,
    // SC: FutTransform<Input=(PublicKey, A), Output=ServerConn> + Clone + Send + 'static,

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
