use clap::ArgMatches;

use app::NodeConnection;

pub enum ConfigError {
}

pub async fn config<'a>(_matches: &'a ArgMatches<'a>, _node_connection: NodeConnection) -> Result<(), ConfigError> {
    unimplemented!();
}
