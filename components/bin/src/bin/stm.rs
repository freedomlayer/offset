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

use std::convert::TryInto;
use std::path::{Path, PathBuf};

use clap::{Arg, App, AppSettings, SubCommand, ArgMatches};
use log::Level;

use crypto::crypto_rand::system_random;
use crypto::identity::{generate_pkcs8_key_pair, Identity};

use proto::net::messages::NetAddressError;
use proto::funder::messages::RelayAddress;
use proto::index_server::messages::IndexServerAddress;
use proto::app_server::messages::AppPermissions;

use database::file_db::FileDb;
use node::NodeState;

use bin::{load_identity_from_file, store_identity_to_file, 
    store_trusted_app_to_file, TrustedApp,
    store_relay_to_file};

#[derive(Debug)]
enum InitNodeDbError {
    OutputAlreadyExists,
    LoadIdentityError,
    FileDbError,
}

fn init_node_db(matches: &ArgMatches) -> Result<(), InitNodeDbError> {
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();
    // Make sure that output_path does not exist.
    // This program should never override any file! (Otherwise users might erase their database by
    // accident).
    let output_path = PathBuf::from(output);
    if output_path.exists() {
        return Err(InitNodeDbError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| InitNodeDbError::LoadIdentityError)?;
    let local_public_key = identity.get_public_key();

    // Create a new database file:
    let initial_state = NodeState::<RelayAddress, IndexServerAddress>::new(local_public_key);
    let _ = FileDb::create(output_path.to_path_buf(), initial_state)
        .map_err(|_| InitNodeDbError::FileDbError)?;

    Ok(())
}

#[derive(Debug)]
enum GenIdentityError {
    StoreToFileError,
}

/// Randomly generate an identity file (private-public key pair)
fn gen_identity(matches: &ArgMatches) -> Result<(), GenIdentityError> {
    // Generate a new random keypair:
    let rng = system_random();
    let pkcs8 = generate_pkcs8_key_pair(&rng);
    let output_path = Path::new(matches.value_of("output").unwrap());

    store_identity_to_file(pkcs8, output_path)
        .map_err(|_| GenIdentityError::StoreToFileError)
}

#[derive(Debug)]
enum AppTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreAppFileError,
}

/// Create an app ticket.
/// The ticket contains:
/// - public key
/// - application permissions
///
/// The app ticket is used to authorize an application to 
/// connect to a running node.
fn app_ticket(matches: &ArgMatches) -> Result<(), AppTicketError> {
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();

    // Make sure that output_path does not exist.
    let output_path = Path::new(output);
    if output_path.exists() {
        return Err(AppTicketError::OutputAlreadyExists);
    }

    // Obtain app's public key:
    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| AppTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    // Get app's permissions:
    let permissions = AppPermissions {
        reports: matches.is_present("preports"),
        routes: matches.is_present("proutes"),
        send_funds: matches.is_present("pfunds"),
        config: matches.is_present("pconfig"),
    };

    // Store app ticket to file:
    let trusted_app = TrustedApp {
        public_key,
        permissions,
    };
    store_trusted_app_to_file(&trusted_app, &output_path)
        .map_err(|_| AppTicketError::StoreAppFileError)
}

#[derive(Debug)]
enum RelayTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreRelayFileError,
    NetAddressError(NetAddressError),
}

impl From<NetAddressError> for RelayTicketError {
    fn from(e: NetAddressError) -> Self {
        RelayTicketError::NetAddressError(e)
    }
}

/// Create a public relay ticket
/// The ticket can be fed into a running offst node
fn relay_ticket(matches: &ArgMatches) -> Result<(), RelayTicketError> {
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();

    // Make sure that output_path does not exist.
    let output_path = Path::new(output);
    if output_path.exists() {
        return Err(RelayTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| RelayTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let address_str = matches.value_of("address").unwrap();

    let relay_address = RelayAddress {
        public_key,
        address: address_str.to_owned().try_into()?,
    };

    store_relay_to_file(&relay_address, &output_path)
        .map_err(|_| RelayTicketError::StoreRelayFileError)
}

#[derive(Debug)]
enum IndexTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreRelayFileError,
    NetAddressError(NetAddressError),
}

impl From<NetAddressError> for IndexTicketError {
    fn from(e: NetAddressError) -> Self {
        IndexTicketError::NetAddressError(e)
    }
}

/// Create a public index ticket
/// The ticket can be fed into a running offst node
fn index_ticket(matches: &ArgMatches) -> Result<(), IndexTicketError> {
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();

    // Make sure that output_path does not exist.
    let output_path = Path::new(output);
    if output_path.exists() {
        return Err(IndexTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| IndexTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let address_str = matches.value_of("address").unwrap();

    let relay_address = RelayAddress {
        public_key,
        address: address_str.to_owned().try_into()?,
    };

    store_relay_to_file(&relay_address, &output_path)
        .map_err(|_| IndexTicketError::StoreRelayFileError)
}

#[derive(Debug)]
enum StmError {
    InitNodeDbError(InitNodeDbError),
    GenIdentityError(GenIdentityError),
    AppTicketError(AppTicketError),
    RelayTicketError(RelayTicketError),
    IndexTicketError(IndexTicketError),
}

impl From<InitNodeDbError> for StmError {
    fn from(e: InitNodeDbError) -> Self {
        StmError::InitNodeDbError(e)
    }
}

impl From<GenIdentityError> for StmError {
    fn from(e: GenIdentityError) -> Self {
        StmError::GenIdentityError(e)
    }
}

impl From<AppTicketError> for StmError {
    fn from(e: AppTicketError) -> Self {
        StmError::AppTicketError(e)
    }
}

impl From<RelayTicketError> for StmError {
    fn from(e: RelayTicketError) -> Self {
        StmError::RelayTicketError(e)
    }
}

impl From<IndexTicketError> for StmError {
    fn from(e: IndexTicketError) -> Self {
        StmError::IndexTicketError(e)
    }
}

fn run() -> Result<(), StmError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("STM: offST Manager")
                          .setting(AppSettings::SubcommandRequiredElseHelp)
                          .version("0.1.0")
                          .author("real <real@freedomlayer.org>")
                          .about("Performs Offst related management operations")
                          .subcommand(SubCommand::with_name("init-node-db")
                              .about("Initialize a new (empty) node database")
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("Database output file path")
                                   .required(true))
                              .arg(Arg::with_name("idfile")
                                   .short("i")
                                   .long("idfile")
                                   .value_name("idfile")
                                   .help("identity file path")
                                   .required(true)))
                          .subcommand(SubCommand::with_name("gen-ident")
                              .about("Randomly generate a new identity file")
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("Identity file output file path")
                                   .required(true)))
                          .subcommand(SubCommand::with_name("app-ticket")
                              .about("Create an application ticket")
                              .arg(Arg::with_name("idfile")
                                   .short("i")
                                   .long("idfile")
                                   .value_name("idfile")
                                   .help("identity file path")
                                   .required(true))
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("Application ticket output file path")
                                   .required(true))
                              .arg(Arg::with_name("preports")
                                   .long("preports")
                                   .value_name("preports")
                                   .help("Permission to receive reports"))
                              .arg(Arg::with_name("proutes")
                                   .long("proutes")
                                   .value_name("proutes")
                                   .help("Permission to request reports"))
                              .arg(Arg::with_name("pfunds")
                                   .long("pfunds")
                                   .value_name("pfunds")
                                   .help("Permission to send funds"))
                              .arg(Arg::with_name("pconfig")
                                   .long("pconfig")
                                   .value_name("pconfig")
                                   .help("Permission to change configuration")))
                          .subcommand(SubCommand::with_name("relay-ticket")
                              .about("Create a relay ticket")
                              .arg(Arg::with_name("idfile")
                                   .short("i")
                                   .long("idfile")
                                   .value_name("idfile")
                                   .help("identity file path")
                                   .required(true))
                              .arg(Arg::with_name("address")
                                   .short("a")
                                   .long("address")
                                   .value_name("address")
                                   .help("Public address of the relay")
                                   .required(true))
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("Relay ticket output file path")
                                   .required(true)))
                          .subcommand(SubCommand::with_name("index-ticket")
                              .about("Create an index server ticket")
                              .arg(Arg::with_name("idfile")
                                   .short("i")
                                   .long("idfile")
                                   .value_name("idfile")
                                   .help("identity file path")
                                   .required(true))
                              .arg(Arg::with_name("address")
                                   .short("a")
                                   .long("address")
                                   .value_name("address")
                                   .help("Public address of the index server")
                                   .required(true))
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("Index server ticket output file path")
                                   .required(true)))
                          .get_matches();

    Ok(match matches.subcommand() {
        ("init-node-db", Some(matches)) => init_node_db(matches)?,
        ("gen-ident", Some(matches)) => gen_identity(matches)?,
        ("app-ticket", Some(matches)) => app_ticket(matches)?,
        ("relay-ticket", Some(matches)) => relay_ticket(matches)?,
        ("index-ticket", Some(matches)) => index_ticket(matches)?,
        _ => unreachable!(),
    })
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}

