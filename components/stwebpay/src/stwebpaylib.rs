use std::io;
use std::path::PathBuf;

use futures::executor::ThreadPool;

use structopt::StructOpt;

use app::{connect, identity_from_file, load_node_from_file};

use crate::web_app::serve_app;

#[derive(Debug)]
pub enum StWebPayError {
    CreateThreadPoolError,
    IdFileDoesNotExist,
    NodeTicketFileDoesNotExist,
    InvalidNodeTicketFile,
    SpawnIdentityServiceError,
    ConnectionError,
}

/// stwebpay: offST WEB PAYment system
/// Pay seamlessly and safely from your browser.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "stwebpay")]
pub struct StWebPayCmd {
    /// StWebPay app identity file path
    #[structopt(parse(from_os_str), short = "I", long = "idfile")]
    pub idfile: PathBuf,
    /// Node ticket file path
    #[structopt(parse(from_os_str), short = "T", long = "ticket")]
    pub node_ticket: PathBuf,
}

pub fn stwebpay(
    st_web_cmd: StWebPayCmd,
    _writer: &mut impl io::Write,
) -> Result<(), StWebPayError> {
    let mut thread_pool = ThreadPool::new().map_err(|_| StWebPayError::CreateThreadPoolError)?;

    let StWebPayCmd {
        idfile,
        node_ticket,
    } = st_web_cmd;

    // Get application's identity:
    if !idfile.exists() {
        return Err(StWebPayError::IdFileDoesNotExist);
    }

    // Get node's connection information (node-ticket):
    if !node_ticket.exists() {
        return Err(StWebPayError::NodeTicketFileDoesNotExist);
    }

    // Get node information from file:
    let node_address =
        load_node_from_file(&node_ticket).map_err(|_| StWebPayError::InvalidNodeTicketFile)?;

    // Spawn identity service:
    let app_identity_client = identity_from_file(&idfile, thread_pool.clone())
        .map_err(|_| StWebPayError::SpawnIdentityServiceError)?;

    let c_thread_pool = thread_pool.clone();
    let _node_connection = thread_pool
        .run(connect(
            node_address.public_key,
            node_address.address,
            app_identity_client,
            c_thread_pool.clone(),
        ))
        .map_err(|_| StWebPayError::ConnectionError)?;

    // Start HTTP server:
    // TODO: Handle errors here:
    serve_app().unwrap();
    Ok(())
}
