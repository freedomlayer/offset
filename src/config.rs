use futures::task::Spawn;
use clap::ArgMatches;

pub enum ConfigError {
}

pub async fn config<'a, S>(_matches: &'a ArgMatches<'a>, _spawner: S) -> Result<(), ConfigError> 
where
    S: Spawn,
{
    unimplemented!();
}
