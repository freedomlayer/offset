use clap::ArgMatches;

use app::NodeConnection;

#[derive(Debug)]
pub enum FundsError {
}

pub async fn funds<'a>(_matches: &'a ArgMatches<'a>, _node_connection: NodeConnection) -> Result<(), FundsError> {
    unimplemented!();
}
