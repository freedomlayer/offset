use super::*;
use futures::executor::ThreadPool;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, future};

use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair};
use crypto::test_utils::DummyRandom;
use identity::{create_identity, IdentityClient};



/// A generator that forwards communication between nodes. Used for testing.
/// Simulates the Channeler interface
async fn dummy_router() {
}


async fn task_funder_basic(identity_clients: Vec<IdentityClient>, spawner: impl Spawn) {
}

#[test]
fn test_funder_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();

    let mut identity_clients = Vec::new();

    for i in 0 .. 6u8 {
        let rng = DummyRandom::new(&[i]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity1);
        let identity_client = IdentityClient::new(requests_sender);
        thread_pool.spawn(identity_server.then(|_| future::ready(()))).unwrap();
        identity_clients.push(identity_client);
    }

    thread_pool.run(task_funder_basic(identity_clients, thread_pool.clone()));
}
