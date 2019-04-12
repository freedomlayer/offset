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

// use std::env;
use std::path::PathBuf;

use futures::executor::ThreadPool;

use derive_more::*;

use structopt::StructOpt;

use stctrl::config::{config, ConfigCmd, ConfigError};
use stctrl::funds::{funds, FundsCmd, FundsError};
use stctrl::info::{info, InfoCmd, InfoError};

use app::{connect, identity_from_file, load_node_from_file};

#[derive(Debug, From)]
enum StCtrlError {
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

#[derive(Debug, StructOpt)]
enum StCtrlSubcommand {
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
#[derive(Debug, StructOpt)]
struct StCtrlCmd {
    /// StCtrl app identity file path
    #[structopt(parse(from_os_str), short = "I", long = "idfile")]
    idfile: PathBuf,
    /// Node ticket file path
    #[structopt(parse(from_os_str), short = "T", name = "ticket")]
    node_ticket: PathBuf,
    #[structopt(flatten)]
    subcommand: StCtrlSubcommand,
}

fn run() -> Result<(), StCtrlError> {
    // simple_logger::init_with_level(Level::Info).unwrap();
    env_logger::init();

    let mut thread_pool = ThreadPool::new().map_err(|_| StCtrlError::CreateThreadPoolError)?;

    let StCtrlCmd {
        idfile,
        node_ticket,
        subcommand,
    } = StCtrlCmd::from_args();

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
                StCtrlSubcommand::Info(info_cmd) => await!(info(info_cmd, node_connection))?,
                StCtrlSubcommand::Config(config_cmd) => {
                    await!(config(config_cmd, node_connection))?
                }
                StCtrlSubcommand::Funds(funds_cmd) => await!(funds(funds_cmd, node_connection))?,
            }
            Ok(())
        },
    )
}

fn main() {
    if let Err(e) = run() {
        error!("error: {:?}", e);
    }
}
