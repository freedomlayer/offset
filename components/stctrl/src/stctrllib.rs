use std::io;
use std::path::PathBuf;

use futures::executor::ThreadPool;

use structopt::StructOpt;

use crate::config::{config, ConfigCmd, ConfigError};
use crate::funds::{funds, FundsCmd, FundsError};
use crate::info::{info, InfoCmd, InfoError};

use app::{connect, identity_from_file, load_node_from_file};

#[derive(Debug)]
pub enum StCtrlError {
    CreateThreadPoolError,
    // MissingIdFileArgument,
    IdFileDoesNotExist,
    // MissingNodeTicketArgument,
    NodeTicketFileDoesNotExist,
    InvalidNodeTicketFile,
    SpawnIdentityServiceError,
    ConnectionError,
    InfoError(InfoError),
    ConfigError(ConfigError),
    FundsError(FundsError),
}

impl From<InfoError> for StCtrlError {
    fn from(e: InfoError) -> Self {
        StCtrlError::InfoError(e)
    }
}

impl From<ConfigError> for StCtrlError {
    fn from(e: ConfigError) -> Self {
        StCtrlError::ConfigError(e)
    }
}

impl From<FundsError> for StCtrlError {
    fn from(e: FundsError) -> Self {
        StCtrlError::FundsError(e)
    }
}

#[derive(Clone, Debug, StructOpt)]
pub enum StCtrlSubcommand {
    #[structopt(name = "info")]
    Info(InfoCmd),
    #[structopt(name = "config")]
    Config(ConfigCmd),
    #[structopt(name = "funds")]
    Funds(FundsCmd),
}

// TODO: Add version (0.1.0)
// TODO: Add author
// TODO: Add description
/// stctrl: offST ConTRoL
#[derive(Clone, Debug, StructOpt)]
pub struct StCtrlCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "I", long = "idfile")]
    pub idfile: PathBuf,
    /// Node ticket file path
    #[structopt(parse(from_os_str), short = "T", name = "ticket")]
    pub node_ticket: PathBuf,
    #[structopt(flatten)]
    pub subcommand: StCtrlSubcommand,
}

pub fn stctrl(st_ctrl_cmd: StCtrlCmd, writer: &mut impl io::Write) -> Result<(), StCtrlError> {
    let mut thread_pool = ThreadPool::new().map_err(|_| StCtrlError::CreateThreadPoolError)?;

    let StCtrlCmd {
        idfile,
        node_ticket,
        subcommand,
    } = st_ctrl_cmd;

    // Get application's identity:
    if !idfile.exists() {
        return Err(StCtrlError::IdFileDoesNotExist);
    }

    // Get node's connection information (node-ticket):
    if !node_ticket.exists() {
        return Err(StCtrlError::NodeTicketFileDoesNotExist);
    }

    // Get node information from file:
    let node_address =
        load_node_from_file(&node_ticket).map_err(|_| StCtrlError::InvalidNodeTicketFile)?;

    // Spawn identity service:
    let app_identity_client = identity_from_file(&idfile, thread_pool.clone())
        .map_err(|_| StCtrlError::SpawnIdentityServiceError)?;

    let c_thread_pool = thread_pool.clone();
    thread_pool.run(
        async move {
            // Connect to node:
            let node_connection = await!(connect(
                node_address.public_key,
                node_address.address,
                app_identity_client,
                c_thread_pool.clone()
            ))
            .map_err(|_| StCtrlError::ConnectionError)?;

            match subcommand {
                StCtrlSubcommand::Info(info_cmd) => await!(info(info_cmd, node_connection, writer))?,
                StCtrlSubcommand::Config(config_cmd) => {
                    await!(config(config_cmd, node_connection))?
                }
                StCtrlSubcommand::Funds(funds_cmd) => await!(funds(funds_cmd, node_connection, writer))?,
            }
            Ok(())
        },
    )
}
