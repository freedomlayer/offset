use std::collections::HashSet;

use futures::StreamExt;

use async_std::fs;
use async_std::path::{Path, PathBuf};

use crate::messages::NodeName;
use crate::store::consts::{NODE_IDENT, DATABASE, APP_IDENT, NODE_INFO, REMOTE, LOCAL};


#[derive(Debug)]
pub enum IntegrityError {
    DuplicateNodeName(NodeName),
    InvalidLocalEntry,
    InvalidRemoteEntry,
    InvalidLocalDir,
    InvalidRemoteDir,
    InvalidNodeDir(PathBuf),
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

async fn verify_local_node(node_path: &Path) -> Result<(), IntegrityError> {
    let mut dir = fs::read_dir(node_path).await.map_err(|_| IntegrityError::InvalidNodeDir(node_path.to_owned()))?;
    let mut node_ident_found = false;
    let mut database_found = false;

    while let Some(res) = dir.next().await {
        let entry = res.map_err(|_| IntegrityError::InvalidNodeDir(node_path.to_owned()))?;
        let filename = entry.file_name().to_string_lossy().to_string();
        match (filename.as_str(), node_ident_found, database_found) {
            (NODE_IDENT, false, _) => node_ident_found = true,
            (DATABASE, _, false) => database_found = true,
            _ => return Err(IntegrityError::InvalidNodeDir(node_path.to_owned())),
        }
    }

    if !(node_ident_found && database_found) {
        return Err(IntegrityError::InvalidNodeDir(node_path.to_owned()));
    }
    Ok(())
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
        verify_local_node(&node_path_buf).await?;
    }
    Ok(())
}

async fn verify_remote_node(node_path: &Path) -> Result<(), IntegrityError> {
    let mut dir = fs::read_dir(node_path).await.map_err(|_| IntegrityError::InvalidNodeDir(node_path.to_owned()))?;
    let mut app_ident_found = false;
    let mut node_info_found = false;

    while let Some(res) = dir.next().await {
        let entry = res.map_err(|_| IntegrityError::InvalidNodeDir(node_path.to_owned()))?;
        let filename = entry.file_name().to_string_lossy().to_string();
        match (filename.as_str(), app_ident_found, node_info_found) {
            (APP_IDENT, false, _) => app_ident_found = true,
            (NODE_INFO, _, false) => node_info_found = true,
            _ => return Err(IntegrityError::InvalidNodeDir(node_path.to_owned())),
        }
    }

    if !(app_ident_found && node_info_found) {
        return Err(IntegrityError::InvalidNodeDir(node_path.to_owned()));
    }
    Ok(())
}

async fn verify_remote_dir(remote_path: &Path, visited_nodes: &mut HashSet<NodeName>) -> Result<(), IntegrityError> {
    let mut dir = fs::read_dir(remote_path).await.map_err(|_| IntegrityError::InvalidRemoteDir)?;
    while let Some(res) = dir.next().await {
        let node_entry = res.map_err(|_| IntegrityError::InvalidRemoteEntry)?;
        let node_path_buf = node_entry.path();

        let node_name = NodeName::new(node_entry.file_name().to_string_lossy().to_string());
        if !visited_nodes.insert(node_name.clone()) {
            // We already have this NodeName
            return Err(IntegrityError::DuplicateNodeName(node_name));
        }
        verify_remote_node(&node_path_buf).await?;
    }
    Ok(())
}

/// Verify store's integrity
pub async fn verify_store(store_path: &Path) -> Result<(), IntegrityError> {

    // All nodes we have encountered so far:
    let mut visited_nodes = HashSet::new();
    let local_path_buf = store_path.join(LOCAL);
    verify_local_dir(&local_path_buf, &mut visited_nodes).await?;
    let remote_path_buf = store_path.join(REMOTE);
    verify_remote_dir(&remote_path_buf, &mut visited_nodes).await?;

    Ok(())
}

