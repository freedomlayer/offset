use futures::task::Spawn;
use clap::ArgMatches;

pub enum InfoError {
}

pub async fn info<'a, S>(_matches: &'a ArgMatches<'a>, _spawner: S) -> Result<(), InfoError> 
where
    S: Spawn,
{
    unimplemented!();
}
