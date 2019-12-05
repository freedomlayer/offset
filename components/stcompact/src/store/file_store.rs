// use futures::StreamExt;
use futures::task::{Spawn, SpawnExt};

use common::conn::BoxFuture;

use async_std::fs::{create_dir_all};
use async_std::path::PathBuf;

use lockfile::{try_lock_file, LockFileHandle};

use crate::messages::{NodeInfo, NodeName};
use crate::store::store::{LoadedNode, NodePrivateInfo, Store};
use crate::store::verify::{verify_store, IntegrityError};


#[allow(unused)]
pub struct FileStore {
    store_path_buf: PathBuf,
    /// If dropped, the advisory lock file protecting the file store will be freed.
    lock_file_handle: LockFileHandle,
}


#[allow(unused)]
#[derive(Debug)]
pub enum FileStoreError {
    IntegrityError(IntegrityError),
    SpawnError,
    LockError,
    CreateDirFailed(PathBuf),
}

const LOCKFILE: &str = "lockfile";

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
        if create_dir_all(&store_path_buf).await.is_err() {
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
    verify_store(&store_path_buf).await.map_err(FileStoreError::IntegrityError)?;

    Ok(FileStore {
        store_path_buf,
        lock_file_handle,
    })
}

#[allow(unused)]
impl Store for FileStore {
    type Error = FileStoreError;

    fn create_node(
        node_private_info: NodePrivateInfo,
    ) -> BoxFuture<'static, Result<(), Self::Error>> {
        unimplemented!();
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
