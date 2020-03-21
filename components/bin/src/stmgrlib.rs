use std::convert::TryInto;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use derive_more::From;

use structopt::StructOpt;

use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::{system_random, RandGen};

use proto::app_server::messages::AppPermissions;
use proto::crypto::PrivateKey;
use proto::net::messages::{NetAddress, NetAddressError};

use database::file_db::FileDb;
use node::NodeState;

use proto::file::{
    IdentityFile, IndexServerFile, NodeAddressFile, RelayAddressFile, TrustedAppFile,
};
use proto::ser_string::{deserialize_from_string, serialize_to_string, StringSerdeError};

#[derive(Debug, From)]
pub enum InitNodeDbError {
    OutputAlreadyExists,
    LoadIdentityError,
    FileDbError,
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
}

#[derive(Debug, StructOpt)]
pub struct InitNodeDbCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Database output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
}

#[derive(Debug, StructOpt)]
pub struct GenIdentCmd {
    /// Identity file output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
}

#[derive(Debug, StructOpt)]
pub struct AppTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Application ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
    /// Permission to request routes
    #[structopt(long = "proutes")]
    pub proutes: bool,
    /// Permission to send funds (buyer)
    #[structopt(long = "pbuyer")]
    pub pbuyer: bool,
    /// Permission to receive funds (seller)
    #[structopt(long = "pseller")]
    pub pseller: bool,
    /// Permission to change configuration
    #[structopt(long = "pconfig")]
    pub pconfig: bool,
}

#[derive(Debug, StructOpt)]
pub struct RelayTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Relay ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
    /// Public address of the relay
    #[structopt(short = "a", long = "address")]
    pub address: String,
}

#[derive(Debug, StructOpt)]
pub struct IndexTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Index server ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
    /// Public address of the index server
    #[structopt(short = "a", long = "address")]
    pub address: String,
}

#[derive(Debug, StructOpt)]
pub struct NodeTicketCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "i", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Node server ticket output file path
    #[structopt(parse(from_os_str), short = "o", long = "output")]
    pub output_path: PathBuf,
    /// Public address of the node server
    #[structopt(short = "a", long = "address")]
    pub address: String,
}

/// stmgr: offSeT ManaGeR
/// A util for managing Offset entities and files
#[derive(Debug, StructOpt)]
#[structopt(name = "stmgr")]
pub enum StMgrCmd {
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

fn init_node_db(
    InitNodeDbCmd {
        idfile_path,
        output_path,
    }: InitNodeDbCmd,
) -> Result<(), InitNodeDbError> {
    // Make sure that output does not exist.
    // This program should never override any file!
    // (Otherwise users might erase their database by
    // accident).
    if output_path.exists() {
        return Err(InitNodeDbError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| InitNodeDbError::LoadIdentityError)?;
    let local_public_key = identity.get_public_key();

    // Create a new database file:
    let initial_state = NodeState::<NetAddress>::new(local_public_key);
    let _ = FileDb::create(output_path, initial_state).map_err(|_| InitNodeDbError::FileDbError)?;

    Ok(())
}

#[derive(Debug, From)]
pub enum GenIdentityError {
    OutputAlreadyExists,
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
}

/// Randomly generate an identity file (private-public key pair)
fn gen_identity(GenIdentCmd { output_path }: GenIdentCmd) -> Result<(), GenIdentityError> {
    // Generate a new random keypair:
    let rng = system_random();
    let private_key = PrivateKey::rand_gen(&rng);

    let identity_file = IdentityFile { private_key };

    if output_path.exists() {
        return Err(GenIdentityError::OutputAlreadyExists);
    }

    let mut file = File::create(output_path)?;
    file.write_all(&serialize_to_string(&identity_file)?.as_bytes())?;

    Ok(())
}

#[derive(Debug, From)]
pub enum AppTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
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
        idfile_path,
        output_path,
        proutes,
        pbuyer,
        pseller,
        pconfig,
    }: AppTicketCmd,
) -> Result<(), AppTicketError> {
    // Obtain app's public key:
    // - Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| AppTicketError::LoadIdentityError)?;

    let public_key = identity.get_public_key();

    // Make sure that output does not exist.
    if output_path.exists() {
        return Err(AppTicketError::OutputAlreadyExists);
    }

    // Get app's permissions:
    let permissions = AppPermissions {
        routes: proutes,
        buyer: pbuyer,
        seller: pseller,
        config: pconfig,
    };

    // Store app ticket to file:
    let trusted_app_file = TrustedAppFile {
        public_key,
        permissions,
    };

    let mut file = File::create(output_path)?;
    file.write_all(&serialize_to_string(&trusted_app_file)?.as_bytes())?;
    Ok(())
}

#[derive(Debug, From)]
pub enum RelayTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    NetAddressError(NetAddressError),
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// Create a public relay ticket
/// The ticket can be fed into a running offset node
fn relay_ticket(
    RelayTicketCmd {
        idfile_path,
        output_path,
        address,
    }: RelayTicketCmd,
) -> Result<(), RelayTicketError> {
    // Make sure that output does not exist.
    if output_path.exists() {
        return Err(RelayTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| RelayTicketError::LoadIdentityError)?;

    let public_key = identity.get_public_key();

    let relay_file = RelayAddressFile {
        public_key,
        address: address.try_into()?,
    };

    let mut file = File::create(output_path)?;
    file.write_all(&serialize_to_string(&relay_file)?.as_bytes())?;
    Ok(())
}

#[derive(Debug, From)]
pub enum IndexTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    StoreIndexFileError,
    NetAddressError(NetAddressError),
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// Create a public index ticket
/// The ticket can be fed into a running offset node
fn index_ticket(
    IndexTicketCmd {
        idfile_path,
        output_path,
        address,
    }: IndexTicketCmd,
) -> Result<(), IndexTicketError> {
    // Make sure that output does not exist.
    if output_path.exists() {
        return Err(IndexTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| IndexTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let index_server_file = IndexServerFile {
        public_key,
        address: address.try_into()?,
    };

    let mut file = File::create(output_path)?;
    file.write_all(&serialize_to_string(&index_server_file)?.as_bytes())?;
    Ok(())
}

#[derive(Debug, From)]
pub enum NodeTicketError {
    OutputAlreadyExists,
    LoadIdentityError,
    NetAddressError(NetAddressError),
    StringSerdeError(StringSerdeError),
    IoError(std::io::Error),
}

/// Create a node ticket
/// The ticket can be fed into a node application
fn node_ticket(
    NodeTicketCmd {
        idfile_path,
        output_path,
        address,
    }: NodeTicketCmd,
) -> Result<(), NodeTicketError> {
    // Make sure that output does not exist.
    if output_path.exists() {
        return Err(NodeTicketError::OutputAlreadyExists);
    }

    // Parse identity file:
    let identity_file: IdentityFile = deserialize_from_string(&fs::read_to_string(&idfile_path)?)?;
    let identity = SoftwareEd25519Identity::from_private_key(&identity_file.private_key)
        .map_err(|_| NodeTicketError::LoadIdentityError)?;
    let public_key = identity.get_public_key();

    let node_address_file = NodeAddressFile {
        public_key,
        address: address.try_into()?,
    };

    let mut file = File::create(output_path)?;
    file.write_all(&serialize_to_string(&node_address_file)?.as_bytes())?;
    Ok(())
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, From)]
pub enum StmError {
    InitNodeDbError(InitNodeDbError),
    GenIdentityError(GenIdentityError),
    AppTicketError(AppTicketError),
    RelayTicketError(RelayTicketError),
    IndexTicketError(IndexTicketError),
    NodeTicketError(NodeTicketError),
}

pub fn stmgr(st_mgr_cmd: StMgrCmd) -> Result<(), StmError> {
    match st_mgr_cmd {
        StMgrCmd::InitNodeDb(i) => init_node_db(i)?,
        StMgrCmd::GenIdent(i) => gen_identity(i)?,
        StMgrCmd::AppTicket(i) => app_ticket(i)?,
        StMgrCmd::RelayTicket(i) => relay_ticket(i)?,
        StMgrCmd::IndexTicket(i) => index_ticket(i)?,
        StMgrCmd::NodeTicket(i) => node_ticket(i)?,
    }

    Ok(())
}
