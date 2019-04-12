#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

use std::convert::TryInto;
use std::path::{Path, PathBuf};

use structopt::StructOpt;

use derive_more::*;

use crypto::crypto_rand::system_random;
use crypto::identity::{generate_pkcs8_key_pair, Identity};

use proto::app_server::messages::{AppPermissions, RelayAddress};
use proto::index_server::messages::IndexServerAddress;
use proto::net::messages::{NetAddress, NetAddressError};
use proto::node::types::NodeAddress;

use database::file_db::FileDb;
use node::NodeState;

use proto::file::app::{store_trusted_app_to_file, TrustedApp};
use proto::file::identity::{load_identity_from_file, store_identity_to_file};
use proto::file::index_server::store_index_server_to_file;
use proto::file::node::store_node_to_file;
use proto::file::relay::store_relay_to_file;

#[derive(Debug)]
enum InitNodeDbError {
    OutputAlreadyExists,
    LoadIdentityError,
    FileDbError,
}

#[derive(Debug, StructOpt)]
struct InitNodeDbCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    idfile: PathBuf,
    /// Database output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
}

#[derive(Debug, StructOpt)]
struct GenIdentCmd {
    /// Identity file output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
}

#[derive(Debug, StructOpt)]
struct AppTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    idfile: PathBuf,
    /// Application ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
    /// Permission to request reports
    #[structopt(long = "proutes")]
    proutes: bool,
    /// Permission to send funds
    #[structopt(long = "pfunds")]
    pfunds: bool,
    /// Permission to change configuration
    #[structopt(long = "pconfig")]
    pconfig: bool,
}

#[derive(Debug, StructOpt)]
struct RelayTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    idfile: PathBuf,
    /// Relay ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
    /// Public address of the relay
    #[structopt(long = "address")]
    address: String,
}

#[derive(Debug, StructOpt)]
struct IndexTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    idfile: PathBuf,
    /// Index server ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
    /// Public address of the index server
    #[structopt(long = "address")]
    address: String,
}

#[derive(Debug, StructOpt)]
struct NodeTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    idfile: PathBuf,
    /// Node server ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    output: PathBuf,
    /// Public address of the node server
    #[structopt(long = "address")]
    address: String,
}

// TODO: Add version (0.1.0)
// TODO: Add author
// TODO: Add description - Performs Offst related management operations
/// stmgr: offST ManaGeR
#[derive(Debug, StructOpt)]
enum StMgrCmd {
    /// Initialize a new (empty) node database
    #[structopt(name = "init-node-db")]
    InitNodeDb(InitNodeDbCmd),
    /// Randomly generate a new identity file
    #[structopt(name = "gen-ident")]
    GenIdent(GenIdentCmd),
    /// Create an application ticket
    #[structopt(name = "app-ticket")]
    AppTicket(AppTicketCmd),
    /// Create a relay ticket
    #[structopt(name = "relay-ticket")]
    RelayTicket(RelayTicketCmd),
    /// Create an index server ticket
    #[structopt(name = "index-ticket")]
    IndexTicket(IndexTicketCmd),
    /// Create a node server ticket
    #[structopt(name = "node-ticket")]
    NodeTicket(NodeTicketCmd),
}

fn init_node_db(InitNodeDbCmd { idfile, output }: InitNodeDbCmd) -> Result<(), InitNodeDbError> {
    // Make sure that output does not exist.
    // This program should never override any file!
    // (Otherwise users might erase their database by
    // accident).
    if output.exists() {
        return Err(InitNodeDbError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity =
        load_identity_from_file(&idfile).map_err(|_| InitNodeDbError::LoadIdentityError)?;
    let local_public_key = identity.get_public_key();

    // Create a new database file:
    let initial_state = NodeState::<NetAddress>::new(local_public_key);
    let _ = FileDb::create(output, initial_state).map_err(|_| InitNodeDbError::FileDbError)?;

    Ok(())
}

#[derive(Debug)]
enum GenIdentityError {
    StoreToFileError,
}

/// Randomly generate an identity file (private-public key pair)
fn gen_identity(GenIdentCmd { output }: GenIdentCmd) -> Result<(), GenIdentityError> {
    // Generate a new random keypair:
    let rng = system_random();
    let pkcs8 = generate_pkcs8_key_pair(&rng);

    store_identity_to_file(pkcs8, &output).map_err(|_| GenIdentityError::StoreToFileError)
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
fn app_ticket(
    AppTicketCmd {
        idfile,
        output,
        proutes,
        pfunds,
        pconfig,
    }: AppTicketCmd,
) -> Result<(), AppTicketError> {
    // Obtain app's public key:
    let identity = load_identity_from_file(Path::new(&idfile))
        .map_err(|_| AppTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    // Make sure that output does not exist.
    if output.exists() {
        return Err(AppTicketError::OutputAlreadyExists);
    }

    // Get app's permissions:
    let permissions = AppPermissions {
        routes: proutes,
        send_funds: pfunds,
        config: pconfig,
    };

    // Store app ticket to file:
    let trusted_app = TrustedApp {
        public_key,
        permissions,
    };
    store_trusted_app_to_file(&trusted_app, &output).map_err(|_| AppTicketError::StoreAppFileError)
}

#[derive(Debug, From)]
enum RelayTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreRelayFileError,
    NetAddressError(NetAddressError),
}

/// Create a public relay ticket
/// The ticket can be fed into a running offst node
fn relay_ticket(
    RelayTicketCmd {
        idfile,
        output,
        address,
    }: RelayTicketCmd,
) -> Result<(), RelayTicketError> {
    // Make sure that output does not exist.
    if output.exists() {
        return Err(RelayTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity =
        load_identity_from_file(&idfile).map_err(|_| RelayTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let relay_address = RelayAddress {
        public_key,
        address: address.try_into()?,
    };

    store_relay_to_file(&relay_address, &output).map_err(|_| RelayTicketError::StoreRelayFileError)
}

#[derive(Debug, From)]
enum IndexTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreIndexFileError,
    NetAddressError(NetAddressError),
}

/// Create a public index ticket
/// The ticket can be fed into a running offst node
fn index_ticket(
    IndexTicketCmd {
        idfile,
        output,
        address,
    }: IndexTicketCmd,
) -> Result<(), IndexTicketError> {
    // Make sure that output does not exist.
    if output.exists() {
        return Err(IndexTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity =
        load_identity_from_file(&idfile).map_err(|_| IndexTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let index_address = IndexServerAddress {
        public_key,
        address: address.try_into()?,
    };

    store_index_server_to_file(&index_address, &output)
        .map_err(|_| IndexTicketError::StoreIndexFileError)
}

#[derive(Debug, From)]
enum NodeTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreNodeFileError,
    NetAddressError(NetAddressError),
}

/// Create a node ticket
/// The ticket can be fed into a node application
fn node_ticket(
    NodeTicketCmd {
        idfile,
        output,
        address,
    }: NodeTicketCmd,
) -> Result<(), NodeTicketError> {
    // Make sure that output does not exist.
    if output.exists() {
        return Err(NodeTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity =
        load_identity_from_file(&idfile).map_err(|_| NodeTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let node_address = NodeAddress {
        public_key,
        address: address.try_into()?,
    };

    store_node_to_file(&node_address, &output).map_err(|_| NodeTicketError::StoreNodeFileError)
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
enum StmError {
    InitNodeDbError(InitNodeDbError),
    GenIdentityError(GenIdentityError),
    AppTicketError(AppTicketError),
    RelayTicketError(RelayTicketError),
    IndexTicketError(IndexTicketError),
    NodeTicketError(NodeTicketError),
}

fn run() -> Result<(), StmError> {
    env_logger::init();

    match StMgrCmd::from_args() {
        StMgrCmd::InitNodeDb(i) => init_node_db(i)?,
        StMgrCmd::GenIdent(i) => gen_identity(i)?,
        StMgrCmd::AppTicket(i) => app_ticket(i)?,
        StMgrCmd::RelayTicket(i) => relay_ticket(i)?,
        StMgrCmd::IndexTicket(i) => index_ticket(i)?,
        StMgrCmd::NodeTicket(i) => node_ticket(i)?,
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        error!("run() error: {:?}", e);
    }
}
