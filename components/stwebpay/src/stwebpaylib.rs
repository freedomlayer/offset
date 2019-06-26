use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use derive_more::From;

use futures::executor::ThreadPool;

use structopt::StructOpt;

use app::file::NodeAddressFile;
use app::ser_string::{deserialize_from_string, StringSerdeError};
use app::{connect, identity_from_file};

use crate::web_app::serve_app;

#[derive(Debug, From)]
pub enum StWebPayError {
    CreateThreadPoolError,
    IdFileDoesNotExist,
    NodeTicketFileDoesNotExist,
    InvalidNodeTicketFile,
    SpawnIdentityServiceError,
    ConnectionError,
    NoBuyerPermissions,
    NoRoutesPermissions,
    GetReportError,
    IoError(std::io::Error),
    StringSerdeError(StringSerdeError),
}

/// stwebpay: offST WEB PAYment system
/// Pay seamlessly and safely from your browser.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "stwebpay")]
pub struct StWebPayCmd {
    /// StWebPay app identity file path
    #[structopt(parse(from_os_str), short = "I", long = "idfile")]
    pub idfile_path: PathBuf,
    /// Node ticket file path
    #[structopt(parse(from_os_str), short = "T", long = "ticket")]
    pub node_ticket_path: PathBuf,
    /// Listening address (For HTTP server)
    #[structopt(short = "l", long = "laddr")]
    pub laddr: SocketAddr,
}

pub fn stwebpay(
    st_web_cmd: StWebPayCmd,
    _writer: &mut impl io::Write,
) -> Result<(), StWebPayError> {
    let mut thread_pool = ThreadPool::new().map_err(|_| StWebPayError::CreateThreadPoolError)?;

    let StWebPayCmd {
        idfile_path,
        node_ticket_path,
        laddr,
    } = st_web_cmd;

    // Get application's identity:
    if !idfile_path.exists() {
        return Err(StWebPayError::IdFileDoesNotExist);
    }

    // Get node's connection information (node-ticket):
    if !node_ticket_path.exists() {
        return Err(StWebPayError::NodeTicketFileDoesNotExist);
    }

    // Get node information from file:
    let node_address_file: NodeAddressFile =
        deserialize_from_string(&fs::read_to_string(&node_ticket_path)?)?;

    // Spawn identity service:
    let app_identity_client = identity_from_file(&idfile_path, thread_pool.clone())
        .map_err(|_| StWebPayError::SpawnIdentityServiceError)?;

    let c_thread_pool = thread_pool.clone();
    let mut app_conn = thread_pool
        .run(connect(
            node_address_file.public_key,
            node_address_file.address,
            app_identity_client,
            c_thread_pool.clone(),
        ))
        .map_err(|_| StWebPayError::ConnectionError)?;

    let app_buyer = app_conn
        .buyer()
        .ok_or(StWebPayError::NoBuyerPermissions)?
        .clone();

    let app_routes = app_conn
        .routes()
        .ok_or(StWebPayError::NoRoutesPermissions)?
        .clone();

    let mut app_report = app_conn.report().clone();
    let (node_report, incoming_mutations) = thread_pool
        .run(app_report.incoming_reports())
        .map_err(|_| StWebPayError::GetReportError)?;
    // We currently don't need live updates about report mutations:
    drop(incoming_mutations);

    let local_public_key = node_report.funder_report.local_public_key.clone();

    // Start HTTP server:
    // TODO: Handle errors here:
    thread_pool
        .run(serve_app(local_public_key, app_buyer, app_routes, laddr))
        .unwrap();
    Ok(())
}
