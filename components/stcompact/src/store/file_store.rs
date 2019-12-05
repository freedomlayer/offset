use std::collections::HashMap;

use futures::StreamExt;
use futures::task::{Spawn, SpawnExt};

use common::conn::BoxFuture;

use async_std::fs;
use async_std::path::{PathBuf, Path};

use lockfile::{try_lock_file, LockFileHandle};

#[allow(unused)]
use app::file::{NodeAddressFile, IdentityFile};
use app::common::{NetAddress, PublicKey, PrivateKey};

use crate::messages::{NodeInfo, NodeName};
#[allow(unused)]
use crate::store::store::{LoadedNode, NodePrivateInfo, Store, NodePrivateInfoLocal, NodePrivateInfoRemote};
use crate::store::consts::{LOCKFILE, LOCAL, REMOTE, DATABASE, NODE_IDENT, NODE_INFO, APP_IDENT};


#[allow(unused)]
pub struct FileStore {
    store_path_buf: PathBuf,
    /// If dropped, the advisory lock file protecting the file store will be freed.
    lock_file_handle: LockFileHandle,
}


#[derive(Debug)]
pub enum FileStoreError {
    DuplicateNodeName(NodeName),
    InvalidLocalDir,
    InvalidRemoteDir,
    InvalidNodeDir(PathBuf),
    SpawnError,
    LockError,
    CreateDirFailed(PathBuf),
    ReadToStringError(PathBuf),
    SerdeError(serde_json::Error),
}

/*
 * File store structure:
 *
 * - lockfile
 * - local [dir]
 *      - node_name1
 *          - node.ident
 *          - database
 * - remote [dir]
 *      - node_name2
 *          - app.ident
 *          - node.info (public_key + address)
*/



pub async fn open_file_store<S>(store_path_buf: PathBuf, file_spawner: &S) -> Result<FileStore, FileStoreError> 
where   
    S: Spawn,
{
    // Create directory if missing:
    if !store_path_buf.is_dir().await {
        if fs::create_dir_all(&store_path_buf).await.is_err() {
            return Err(FileStoreError::CreateDirFailed(store_path_buf.into()));
        }
    }

    let store_path_buf_std: std::path::PathBuf = store_path_buf.clone().into();
    let lockfile_path_buf = store_path_buf_std.join(LOCKFILE);
    let lock_file_handle = file_spawner.spawn_with_handle(async move {
        try_lock_file(&lockfile_path_buf)
    }).map_err(|_| FileStoreError::SpawnError)?
    .await
    .map_err(|_| FileStoreError::LockError)?;

    // Verify file store's integrity:
    verify_store(&store_path_buf).await?;

    Ok(FileStore {
        store_path_buf,
        lock_file_handle,
    })
}

#[derive(Debug, Clone)]
struct FileStoreNodeLocal {
    node_private_key: PrivateKey,
    node_database: PathBuf,
}

#[derive(Debug, Clone)]
struct FileStoreNodeRemote {
    app_private_key: PrivateKey,
    node_public_key: PublicKey,
    node_address: NetAddress,
}

#[derive(Debug, Clone)]
enum FileStoreNode {
    Local(FileStoreNodeLocal),
    Remote(FileStoreNodeRemote),
}

type FileStoreNodes = HashMap<NodeName, FileStoreNode>;


async fn read_local_node(local_path: &Path) -> Result<FileStoreNodeLocal, FileStoreError> {
    let node_ident_path = local_path.join(NODE_IDENT);
    let ident_data = fs::read_to_string(&node_ident_path).await.map_err(|_| FileStoreError::ReadToStringError(node_ident_path))?;
    let identity_file: IdentityFile = serde_json::from_str(&ident_data).map_err(FileStoreError::SerdeError)?;

    Ok(FileStoreNodeLocal {
        node_private_key: identity_file.private_key,
        node_database: local_path.join(DATABASE),
    })
}

async fn read_remote_node(remote_path: &Path) -> Result<FileStoreNodeRemote, FileStoreError> {
    let app_ident_path = remote_path.join(APP_IDENT);
    let ident_data = fs::read_to_string(&app_ident_path).await.map_err(|_| FileStoreError::ReadToStringError(app_ident_path))?;
    let identity_file: IdentityFile = serde_json::from_str(&ident_data).map_err(FileStoreError::SerdeError)?;

    let node_info_path = remote_path.join(NODE_INFO);
    let node_info_data = fs::read_to_string(&node_info_path).await.map_err(|_| FileStoreError::ReadToStringError(node_info_path))?;
    let node_address_file: NodeAddressFile = serde_json::from_str(&node_info_data).map_err(FileStoreError::SerdeError)?;

    Ok(FileStoreNodeRemote {
        app_private_key: identity_file.private_key,
        node_public_key: node_address_file.public_key,
        node_address: node_address_file.address,
    })
}

async fn read_all_nodes(store_path: &Path) -> Result<FileStoreNodes, FileStoreError> {
    let mut file_store_nodes = HashMap::new();

    // Read local nodes:
    let local_dir = store_path.join(LOCAL);
    let mut dir = fs::read_dir(local_dir)
        .await
        .map_err(|_| FileStoreError::InvalidLocalDir)?;

    while let Some(res) = dir.next().await {
        let local_node_entry = res.map_err(|_| FileStoreError::InvalidLocalDir)?;
        let node_name = NodeName::new(local_node_entry.file_name().to_string_lossy().to_string());
        read_local_node(&local_node_entry.path()).await?;
        let local_node = FileStoreNode::Local(read_local_node(&local_node_entry.path()).await?);
        if file_store_nodes.insert(node_name.clone(), local_node).is_some() {
            return Err(FileStoreError::DuplicateNodeName(node_name));
        }
    }

    // Read remote nodes:
    let remote_dir = store_path.join(REMOTE);
    let mut dir = fs::read_dir(remote_dir)
        .await
        .map_err(|_| FileStoreError::InvalidRemoteDir)?;

    while let Some(res) = dir.next().await {
        let remote_node_entry = res.map_err(|_| FileStoreError::InvalidRemoteDir)?;
        let node_name = NodeName::new(remote_node_entry.file_name().to_string_lossy().to_string());
        let remote_node = FileStoreNode::Remote(read_remote_node(&remote_node_entry.path()).await?);
        if file_store_nodes.insert(node_name.clone(), remote_node).is_some() {
            return Err(FileStoreError::DuplicateNodeName(node_name));
        }
    }

    Ok(file_store_nodes)
}

/// Verify store's integrity
pub async fn verify_store(store_path: &Path) -> Result<(), FileStoreError> {
    // We read all nodes, and make sure it works correctly. We discard the result.
    let _ = read_all_nodes(store_path).await?;
    Ok(())
}


async fn _create_local_node(_info_local: NodePrivateInfoLocal, _local_path: &Path) {
    unimplemented!();
}

#[allow(unused)]
impl Store for FileStore {
    type Error = FileStoreError;

    fn create_node(
        node_private_info: NodePrivateInfo,
    ) -> BoxFuture<'static, Result<(), Self::Error>> {
        Box::pin(async move {
            match node_private_info {
                NodePrivateInfo::Local(local) => unimplemented!(),
                NodePrivateInfo::Remote(remote) => unimplemented!(),
            }
        })
    }

    fn list_nodes(&self) -> BoxFuture<'static, Result<Vec<NodeInfo>, Self::Error>> {
        unimplemented!();
    }

    fn load_node(
        &mut self,
        node_name: &NodeName,
    ) -> BoxFuture<'static, Result<LoadedNode, Self::Error>> {
        unimplemented!();
    }

    /// Unload a node
    fn unload_node(&mut self, node_name: &NodeName) -> BoxFuture<'static, Result<(), Self::Error>> {
        unimplemented!();
    }

    /// Remove a node from the store
    /// A node must be in unloaded state to be removed.
    fn remove_node(&mut self, node_name: &NodeName) -> BoxFuture<'static, Result<(), Self::Error>> {
        unimplemented!();
    }
}
