use std::convert::TryFrom;

use futures::executor::{block_on, ThreadPool};
use futures::task::Spawn;

use crypto::test_utils::DummyRandom;
use crypto::rand::RandGen;
use crypto::identity::derive_public_key;

use proto::crypto::PrivateKey;
use proto::net::messages::NetAddress;

use crate::messages::NodeName;
use crate::store::file_store::open_file_store;
use crate::store::store::Store;


use tempfile::tempdir;

async fn task_file_store<S,FS>(spawner: S, file_spawner: FS) 
where
    S: Spawn + Send + Sync,
    FS: Spawn + Clone + Send + Sync + 'static,
{
    let store_dir = tempdir().unwrap();
    let mut file_store = open_file_store(store_dir.path().into(), spawner, file_spawner)
        .await
        .unwrap();

    let nodes_info = file_store.list_nodes().await.unwrap();
    assert!(nodes_info.is_empty());

    let rng = DummyRandom::new(&[1u8]);
    let node0_private_key = PrivateKey::rand_gen(&rng);
    let node1_private_key = PrivateKey::rand_gen(&rng);
    let node2_private_key = PrivateKey::rand_gen(&rng);

    file_store.create_local_node(NodeName::new("node0".to_owned()), node0_private_key).await.unwrap();
    file_store.create_local_node(NodeName::new("node1".to_owned()), node1_private_key).await.unwrap();
    file_store.create_local_node(NodeName::new("node2".to_owned()), node2_private_key).await.unwrap();

    let nodes_info = file_store.list_nodes().await.unwrap();
    assert_eq!(nodes_info.len(), 3);

    let app_private_key = PrivateKey::rand_gen(&rng);
    let node_public_key = derive_public_key(&PrivateKey::rand_gen(&rng)).unwrap();
    let node_address = NetAddress::try_from("node_address".to_owned()).unwrap();

    file_store.create_remote_node(NodeName::new("node3".to_owned()), 
        app_private_key,
        node_public_key,
        node_address).await.unwrap();

    let nodes_info = file_store.list_nodes().await.unwrap();
    assert_eq!(nodes_info.len(), 4);

    file_store.remove_node(NodeName::new("node0".to_owned())).await.unwrap();

    let nodes_info = file_store.list_nodes().await.unwrap();
    assert_eq!(nodes_info.len(), 3);

    // Load/unload local node:
    let loaded_node = file_store.load_node(NodeName::new("node1".to_owned())).await.unwrap();

    // Should not be possible to remove a node while it is loaded:
    let res = file_store.remove_node(NodeName::new("node1".to_owned())).await;
    assert!(res.is_err());

    drop(loaded_node);
    file_store.unload_node(&NodeName::new("node1".to_owned())).await.unwrap();

    // Load/unload remote node:
    let loaded_node = file_store.load_node(NodeName::new("node3".to_owned())).await.unwrap();

    // Should not be possible to remove a node while it is loaded:
    let res = file_store.remove_node(NodeName::new("node3".to_owned())).await;
    assert!(res.is_err());

    drop(loaded_node);
    file_store.unload_node(&NodeName::new("node3".to_owned())).await.unwrap();

    // Remove all remaining nodes:
    file_store.remove_node(NodeName::new("node1".to_owned())).await.unwrap();
    file_store.remove_node(NodeName::new("node2".to_owned())).await.unwrap();
    file_store.remove_node(NodeName::new("node3".to_owned())).await.unwrap();
}

#[test]
fn test_file_store() {
    let spawner = ThreadPool::new().unwrap();
    let file_spawner = ThreadPool::new().unwrap();
    block_on(task_file_store(spawner, file_spawner))
}
