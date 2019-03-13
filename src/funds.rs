use futures::task::Spawn;
use clap::ArgMatches;


pub enum FundsError {
}

pub async fn funds<'a, S>(_matches: &'a ArgMatches<'a>, _spawner: S) -> Result<(), FundsError> 
where
    S: Spawn,
{
    unimplemented!();
}
