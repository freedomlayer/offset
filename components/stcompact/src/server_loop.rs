use std::fmt::Debug;
use std::collections::HashMap;

use futures::{stream, StreamExt, SinkExt, channel::mpsc, future, Sink};
use futures::task::Spawn;

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

#[allow(unused)]
use app::common::{derive_public_key, Uid};

#[allow(unused)]
use crate::messages::{ServerToUser, UserToServer, ServerToUserAck, UserToServerAck, NodeId, NodeName, 
    RequestCreateNode, NodeInfo, NodeInfoLocal, NodeInfoRemote, CreateNodeLocal, CreateNodeRemote, 
    NodeStatus, NodesStatus, ResponseOpenNode};
#[allow(unused)]
use crate::compact_node::{CompactToUser, CompactToUserAck, UserToCompact, UserToCompactAck};
use crate::gen::GenPrivateKey;
use crate::store::{Store, LoadedNode};

pub type ConnPairServer = ConnPair<ServerToUserAck, UserToServerAck>;

#[allow(unused)]
#[derive(Debug)]
pub enum ServerError { 
    UserSenderError,
    NodeSenderError,
    DerivePublicKeyError,
    StoreError,
}

type CompactNodeEvent = (NodeId, CompactToUserAck);

pub enum ServerEvent {
    User(UserToServerAck),
    UserClosed,
    CompactNode(CompactNodeEvent),
}

#[derive(Debug)]
pub struct OpenNode {
    pub node_name: NodeName,
    pub sender: mpsc::Sender<UserToCompactAck>,
    // TODO: How to signal to close the node?
    // Possibly await some handle (If we use spawn_with_handle for example?)
}

#[derive(Debug)]
pub struct ServerState<ST> {
    pub next_node_id: NodeId,
    pub open_nodes: HashMap<NodeId, OpenNode>,
    pub store: ST,
}

impl<ST> ServerState<ST> {
    pub fn new(store: ST) -> Self {
        Self {
            next_node_id: NodeId(0),
            open_nodes: HashMap::new(),
            store,
        }
    }

    fn is_node_open(&self, node_name: &NodeName) -> bool {
        for open_node in self.open_nodes.values() {
            if &open_node.node_name == node_name {
                return true;
            }
        }
        false
    }
}

#[allow(unused)]
pub async fn handle_create_node_local<ST,CG>(
    create_node_local: CreateNodeLocal, 
    store: &mut ST, 
    compact_gen: &mut CG) -> Result<bool, ServerError> 

where
    ST: Store,
    CG: GenPrivateKey,
{
    // Randomly generate a private key ourselves, because we don't trust the user to correctly randomly
    // generate a private key.
    let node_private_key = compact_gen.gen_private_key();
    let node_public_key = derive_public_key(&node_private_key).map_err(|_| ServerError::DerivePublicKeyError)?;
    Ok(if let Err(e) = store.create_local_node(create_node_local.node_name.clone(), node_private_key).await {
        warn!("handle_create_node_local: store error: {:?}", e);
        false
    } else {
        true
    })
}

pub async fn handle_create_node_remote<ST>(
    create_node_remote: CreateNodeRemote, 
    store: &mut ST) -> Result<bool, ServerError> 
where
    ST: Store,
{
    /*
    let app_public_key = derive_public_key(&create_node_remote.app_private_key)
                    .map_err(|_| ServerError::DerivePublicKeyError)?;
    */
    let create_remote_node_res = store.create_remote_node(
        create_node_remote.node_name.clone(),
        create_node_remote.app_private_key,
        create_node_remote.node_public_key.clone(),
        create_node_remote.node_address.clone()).await;

    Ok(if let Err(e) = create_remote_node_res {
        warn!("handle_create_node_remote: store error: {:?}", e);
        false
    } else {
        true
    })
}

/// Returns if any change has happened
async fn handle_create_node<CG, ST>(
    request_create_node: RequestCreateNode,
    server_state: &mut ServerState<ST>,
    compact_gen: &mut CG) -> Result<bool, ServerError> 
where
    CG: GenPrivateKey,
    ST: Store,
{

    Ok(match request_create_node {
        RequestCreateNode::CreateNodeLocal(local) => {
            if server_state.is_node_open(&local.node_name) {
                warn!("handle_create_node: Node {:?} is already open!", local.node_name);
                return Ok(false);
            }
            handle_create_node_local(local, &mut server_state.store, compact_gen).await?
        }
        RequestCreateNode::CreateNodeRemote(remote) => {
            if server_state.is_node_open(&remote.node_name) {
                warn!("handle_create_node: Node {:?} is already open!", remote.node_name);
                return Ok(false);
            }
            handle_create_node_remote(remote, &mut server_state.store).await?
        }
    })
}

pub async fn handle_remove_node<ST>(
    node_name: NodeName, 
    server_state: &mut ServerState<ST>) -> Result<bool, ServerError> 
where
    ST: Store,
{
    // Make sure that we do not attempt to remove an open node:
    if server_state.is_node_open(&node_name) {
        warn!("handle_remove_node(): node {:?} is open", node_name);
        return Ok(false);
    }


    let remove_res = server_state.store.remove_node(node_name.clone()).await;
    Ok(if let Err(e) = remove_res {
        warn!("handle_remove_node(): store error: {:?}", e);
        false
    } else {
        true
    })
}

async fn handle_close_node<ST>(node_id: &NodeId, server_state: &mut ServerState<ST>) -> Result<bool, ServerError> 
where
    ST: Store,
{
    // Remove from open nodes:
    let open_node = if let Some(open_node) = server_state.open_nodes.remove(&node_id) {
        open_node
    } else {
        return Ok(false);
    };

    // TODO: Make sure we drop everything besides the name:
    let OpenNode {
        node_name,
        ..
    } = open_node;

    // Unload from store:
    if let Err(e) = server_state.store.unload_node(&node_name).await {
        warn!("handle_close_node(): Error in unload_node(): {:?}", e);
    }

    Ok(true)
}

#[allow(unused)]
async fn handle_open_node<ST,US,S>(node_name: NodeName, server_state: &mut ServerState<ST>, user_sender: &mut US, _spawner: &S) -> Result<bool, ServerError> 
where
    ST: Store,
    US: Sink<ServerToUserAck> + Unpin,
    S: Spawn,
{
    // Load node from store:
    let loaded_node = match server_state.store.load_node(node_name.clone()).await {
        Ok(loaded_node) => loaded_node,
        Err(e) => {
            let response_open_node = ResponseOpenNode::Failure(node_name);
            let server_to_user = ServerToUser::ResponseOpenNode(response_open_node);
            user_sender.send(ServerToUserAck::ServerToUser(server_to_user)).await.map_err(|_| ServerError::UserSenderError)?;
            warn!("handle_open_node: load_node() error: {:?}",e);
            return Ok(false);
        }
    };


    match loaded_node {
        LoadedNode::Local(local) => {
            // TODO: Spawn a new node
            unimplemented!();
        },
        LoadedNode::Remote(remote) => {
            // TODO: Connect to a remote node
            unimplemented!();
        }
    }

    // TODO:
    // - Open node, should be different between local, remote
    // - Send ResponseOpenNode, with relevant first CompactReport
    unimplemented!();

}


async fn build_nodes_status<ST>(server_state: &mut ServerState<ST>) -> Result<NodesStatus, ServerError> 
where
    ST: Store,
{
    let nodes_info = server_state.store.list_nodes().await.map_err(|_| ServerError::StoreError)?;
    Ok(nodes_info.into_iter().map(|(node_name, node_info)| (node_name.clone(), NodeStatus {
        is_open: server_state.is_node_open(&node_name),
        info: node_info,
    })).collect())
}


#[allow(unused)]
pub async fn handle_user_to_server<S,ST,CG,US>(
    user_to_server_ack: UserToServerAck, 
    server_state: &mut ServerState<ST>,
    compact_gen: &mut CG,
    user_sender: &mut US,
    spawner: &S) -> Result<(), ServerError> 
where
    S: Spawn,
    ST: Store,
    CG: GenPrivateKey,
    US: Sink<ServerToUserAck> + Unpin,
{
    let UserToServerAck {
        request_id,
        inner: user_to_server,
    } = user_to_server_ack;

    let has_changed = match user_to_server {
        UserToServer::RequestCreateNode(request_create_node) => handle_create_node(request_create_node, server_state, compact_gen).await?,
        UserToServer::RequestRemoveNode(node_name) => handle_remove_node(node_name, server_state).await?,
        UserToServer::RequestOpenNode(node_name) => handle_open_node(node_name, server_state, user_sender, spawner).await?, 
        UserToServer::RequestCloseNode(node_id) => handle_close_node(&node_id, server_state).await?,
        UserToServer::Node(node_id, user_to_compact) => {
            let node_state = if let Some(node_state) = server_state.open_nodes.get_mut(&node_id) {
                node_state
            } else {
                warn!("UserToServer::Node: nonexistent node {:?}", node_id);
                return Ok(());
            };
            let user_to_compact_ack = UserToCompactAck {
                user_request_id: request_id,
                inner: user_to_compact,
            };
            return node_state.sender.send(user_to_compact_ack).await.map_err(|_| ServerError::NodeSenderError);
        },
    };

    // We get here if not of type UserToServer::Node(...):

    // If any change happened to NodesStatus, send the new version of NodesStatus to the user:
    if has_changed {
        let nodes_status_map = build_nodes_status(server_state).await?;
        let nodes_status = ServerToUser::NodesStatus(nodes_status_map);
        let server_to_user_ack = ServerToUserAck::ServerToUser(nodes_status);
        user_sender.send(server_to_user_ack).await.map_err(|_| ServerError::NodeSenderError)?;
    }

    // Send ack to the user (In the case of UserToServer::Node, another mechanism sends the ack):
    let server_to_user_ack = ServerToUserAck::Ack(request_id);
    user_sender.send(server_to_user_ack).await.map_err(|_| ServerError::NodeSenderError)
}

#[allow(unused)]
pub async fn inner_server_loop<ST,CG,S>(
    conn_pair: ConnPairServer,
    store: ST,
    mut compact_gen: CG,
    spawner: S,
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), ServerError> 
where
    ST: Store,
    CG: GenPrivateKey,
    S: Spawn,
{

    let mut server_state = ServerState::new(store);

    let (mut user_sender, mut user_receiver) = conn_pair.split();

    let user_receiver = user_receiver.map(ServerEvent::User)
        .chain(stream::once(future::ready(ServerEvent::UserClosed)));

    let (compact_node_sender, compact_node_receiver) = mpsc::channel(1);
    let compact_node_receiver = compact_node_receiver.map(ServerEvent::CompactNode);

    let mut incoming_events = select_streams![
        user_receiver,
        compact_node_receiver
    ];

    while let Some(event) = incoming_events.next().await {
        match event {
            ServerEvent::User(user_to_server) => {
                handle_user_to_server(user_to_server, &mut server_state, &mut compact_gen, &mut user_sender, &spawner).await?;
            }
            ServerEvent::UserClosed => return Ok(()),
            ServerEvent::CompactNode(compact_node_event) => {
                let (node_id, compact_to_user_ack) = compact_node_event;
                match compact_to_user_ack {
                    CompactToUserAck::Ack(request_id) => {
                        user_sender.send(ServerToUserAck::Ack(request_id))
                            .await
                            .map_err(|_| ServerError::UserSenderError)?;
                    },
                    CompactToUserAck::CompactToUser(compact_to_user) => {
                        if server_state.open_nodes.contains_key(&node_id) {
                            let server_to_user = ServerToUser::Node(node_id, compact_to_user);
                            user_sender.send(ServerToUserAck::ServerToUser(server_to_user))
                                .await
                                .map_err(|_| ServerError::UserSenderError)?;
                        } else {
                            // The user was told that this node is closed,
                            // so we do not want to forward any more messages from this node to the user.
                            warn!("inner_server_loop: compact_node_event from a node {:?} that is closed.", node_id);
                        }
                    },
                }
            },
        }
        // For testing:
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = event_sender.send(()).await;
        }
    }
    Ok(())
}

// pub async fn server_loop() -> Result<(), ServerError> {}
