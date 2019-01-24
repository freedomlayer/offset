use futures::task::Spawn;
use futures::Stream;

use database::DatabaseClient;
use identity::IdentityClient;
use crypto::crypto_rand::CryptoRandom;

use crate::types::NodeMutation;

#[derive(Debug)]
pub enum NodeError {
}

pub struct NodeConfig {
}

pub async fn node_loop<B,ISA,IA,R,S>(
                node_config: NodeConfig,
                identity_client: IdentityClient,
                database_client: DatabaseClient<NodeMutation<B,ISA>>,
                incoming_apps: IA,
                rng: R,
                spawner: S) -> Result<(), NodeError> 
where
    R: CryptoRandom,
    S: Spawn,
    IA: Stream, // TODO: Add type of Item
{

    unimplemented!();
}
