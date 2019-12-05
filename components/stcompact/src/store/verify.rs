use std::collections::HashSet;

use futures::StreamExt;

use async_std::fs;
use async_std::path::{Path};

use crate::messages::NodeName;

#[allow(unused)]
#[derive(Debug)]
pub enum IntegrityError {
    DuplicateNodeName(NodeName),
    LocalMissingNodeIdent(NodeName),
    LocalMissingDb(NodeName),
    RemoteMissingAppIdent(NodeName),
    RemoteMissingNodeInfo(NodeName),
    RootDirMissing,
    InvalidLocalEntry,
    InvalidRemoteEntry,
    InvalidLocalDir,
    InvalidRemoteDir,
}


async fn verify_local_node(_node_path_buf: &Path, _node_name: &NodeName) -> Result<(), IntegrityError> {
    unimplemented!();
}

async fn verify_local_dir(local_path: &Path, visited_nodes: &mut HashSet<NodeName>) -> Result<(), IntegrityError> {
    let mut dir = fs::read_dir(local_path).await.map_err(|_| IntegrityError::InvalidLocalDir)?;
    while let Some(res) = dir.next().await {
        let node_entry = res.map_err(|_| IntegrityError::InvalidLocalEntry)?;
        let node_path_buf = node_entry.path();

        let node_name = NodeName::new(node_entry.file_name().to_string_lossy().to_string());
        if !visited_nodes.insert(node_name.clone()) {
            // We already have this NodeName
            return Err(IntegrityError::DuplicateNodeName(node_name));
        }
        verify_local_node(&node_path_buf, &node_name).await?;
    }
    Ok(())
}

async fn verify_remote_dir(_remote_path: &Path, _visited_nodes: &mut HashSet<NodeName>) -> Result<(), IntegrityError> {
    unimplemented!();
}

/// Verify store's integrity
pub async fn verify_store(store_path: &Path) -> Result<(), IntegrityError> {

    // All nodes we have encountered so far:
    let mut visited_nodes = HashSet::new();
    let local_path_buf = store_path.join("local");
    verify_local_dir(&local_path_buf, &mut visited_nodes).await?;
    let remote_path_buf = store_path.join("remote");
    verify_remote_dir(&remote_path_buf, &mut visited_nodes).await?;

    Ok(())
}

