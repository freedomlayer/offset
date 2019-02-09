#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::path::{Path, PathBuf};

use clap::{Arg, App, SubCommand, ArgMatches};
use log::Level;

use crypto::crypto_rand::system_random;
use crypto::identity::{generate_pkcs8_key_pair, Identity};

use proto::funder::messages::RelayAddress;
use proto::index_server::messages::IndexServerAddress;
use proto::app_server::messages::AppPermissions;

use database::file_db::FileDb;
use node::NodeState;

use bin::{load_identity_from_file, store_identity_to_file, 
    store_trusted_app_to_file, TrustedApp};

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
    let file_db = FileDb::create(output_path.to_path_buf(), initial_state)
        .map_err(|_| InitNodeDbError::FileDbError)?;

    Ok(())
}

#[derive(Debug)]
enum GenIdentityError {
    StoreToFileError,
}

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

fn app_ticket(matches: &ArgMatches) -> Result<(), AppTicketError> {
    let idfile = matches.value_of("idfile").unwrap();
    let output = matches.value_of("output").unwrap();
    // Make sure that output_path does not exist.
    // This program should never override any file! (Otherwise users might erase their database by
    // accident).
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
        .map_err(|_| AppTicketError::StoreAppFileError)?;

    Ok(())
}

#[derive(Debug)]
enum StmError {
    InitNodeDbError(InitNodeDbError),
    GenIdentityError(GenIdentityError),
    AppTicketError(AppTicketError),
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

fn run() -> Result<(), StmError> {
    simple_logger::init_with_level(Level::Warn).unwrap();
    let matches = App::new("STM: offST Manager")
                          .version("0.1.0")
                          .author("real <real@freedomlayer.org>")
                          .about("Performs Offst related management operations")
                          .subcommand(SubCommand::with_name("init-node-db")
                              .about("Initialize a new (empty) node database")
                              .arg(Arg::with_name("output")
                                   .short("o")
                                   .long("output")
                                   .value_name("output")
                                   .help("output database file path")
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
                                   .help("output identity file path")
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
                                   .help("output application ticket file path")
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
                          .get_matches();

    Ok(match matches.subcommand() {
        ("init-node-db", Some(matches)) => init_node_db(matches)?,
        ("gen-ident", Some(matches)) => gen_identity(matches)?,
        ("app-ticket", Some(matches)) => app_ticket(matches)?,
        ("relay-ticket", Some(matches)) => unimplemented!(),
        ("index-ticket", Some(matches)) => unimplemented!(),
        _ => unreachable!(),
    })
}


fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}

