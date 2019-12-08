use std::fmt::Debug;
use std::collections::HashMap;

use futures::{stream, StreamExt, SinkExt, channel::mpsc, future, Sink};
use futures::task::Spawn;

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

use app::common::derive_public_key;

#[allow(unused)]
use crate::messages::{ServerToUser, UserToServer, NodeId, NodeName, 
    RequestCreateNode, CreateNodeResult, NodeInfo, NodeInfoLocal, NodeInfoRemote, CreateNodeLocal, CreateNodeRemote};
#[allow(unused)]
use crate::compact_node::{CompactToUser, CompactToUserAck, UserToCompact, UserToCompactAck};
use crate::gen::GenPrivateKey;
use crate::store::Store;

pub type ConnPairServer = ConnPair<ServerToUser, UserToServer>;

#[allow(unused)]
#[derive(Debug)]
pub enum ServerError { 
    UserSenderError,
    NodeSenderError,
    DerivePublicKeyError,
    NodeIsOpen,
}

type CompactNodeEvent = (NodeId, CompactToUserAck);

pub enum ServerEvent {
    User(UserToServer),
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
pub async fn handle_create_node_local<ST,US,CG>(
    create_node_local: CreateNodeLocal, 
    store: &mut ST, 
    user_sender: &mut US, 
    compact_gen: &mut CG) -> Result<(), ServerError> 
where
    ST: Store,
    US: Sink<ServerToUser> + Unpin,
    CG: GenPrivateKey,
{
    unimplemented!();
    /*
    // Randomly generate a private key ourselves, because we don't trust the user to correctly randomly
    // generate a private key.
    let node_private_key = compact_gen.gen_private_key();
    let node_public_key = derive_public_key(&node_private_key).map_err(|_| ServerError::DerivePublicKeyError)?;
    match store.create_local_node(create_node_local.node_name.clone(), node_private_key).await {
        Ok(()) => {
            let node_info_local = NodeInfoLocal {
                node_public_key,
            };
            let node_info = NodeInfo::Local(node_info_local);
            let create_node_result = CreateNodeResult::Success(node_info);
            user_sender.send(ServerToUser::ResponseCreateNode(create_node_local.node_name, create_node_result))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        },
        Err(e) => {
            warn!("handle_create_node_local: store error: {:?}", e);
            user_sender.send(ServerToUser::ResponseCreateNode(create_node_local.node_name, CreateNodeResult::Failure))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        },
    }
    Ok(())
    */
}

#[allow(unused)]
pub async fn handle_create_node_remote<ST,US>(
    create_node_remote: CreateNodeRemote, 
    store: &mut ST, 
    user_sender: &mut US) -> Result<(), ServerError> 
where
    ST: Store,
    US: Sink<ServerToUser> + Unpin,
{

    let app_public_key = derive_public_key(&create_node_remote.app_private_key)
                    .map_err(|_| ServerError::DerivePublicKeyError)?;
    let create_remote_node_res = store.create_remote_node(
        create_node_remote.node_name.clone(),
        create_node_remote.app_private_key,
        create_node_remote.node_public_key.clone(),
        create_node_remote.node_address.clone()).await;

    unimplemented!();

    /*
    match create_remote_node_res {
        Ok(()) => {
            let node_info_remote = NodeInfoRemote {
                app_public_key,
                node_public_key: create_node_remote.node_public_key,
                node_address: create_node_remote.node_address,
            };
            let node_info = NodeInfo::Remote(node_info_remote);
            let create_node_result = CreateNodeResult::Success(node_info);
            user_sender.send(ServerToUser::ResponseCreateNode(create_node_remote.node_name, create_node_result))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        },
        Err(e) => {
            warn!("handle_create_node_remote: store error: {:?}", e);
            user_sender.send(ServerToUser::ResponseCreateNode(create_node_remote.node_name, CreateNodeResult::Failure))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        },
    }
    Ok(())
    */
}

#[allow(unused)]
async fn handle_create_node<CG, US, ST>(
    request_create_node: RequestCreateNode,
    server_state: &mut ServerState<ST>,
    compact_gen: &mut CG,
    mut user_sender: &mut US) -> Result<(), ServerError> 
where
    CG: GenPrivateKey,
    US: Sink<ServerToUser> + Unpin,
    ST: Store,
{

    match request_create_node {
        RequestCreateNode::CreateNodeLocal(local) => {
            if server_state.is_node_open(&local.node_name) {
                return Err(ServerError::NodeIsOpen);
            }
            handle_create_node_local(local, &mut server_state.store, &mut user_sender, compact_gen).await?;
        }
        RequestCreateNode::CreateNodeRemote(remote) => {
            if server_state.is_node_open(&remote.node_name) {
                return Err(ServerError::NodeIsOpen);
            }
            handle_create_node_remote(remote, &mut server_state.store, &mut user_sender).await?;
        }
    }
    Ok(())
}

#[allow(unused)]
pub async fn handle_remove_node<US,ST>(
    node_name: NodeName, 
    server_state: &mut ServerState<ST>, 
    user_sender: &mut US) -> Result<(), ServerError> 
where
    US: Sink<ServerToUser> + Unpin,
    ST: Store,
{
    // Make sure that we do not attempt to remove an open node:
    if server_state.is_node_open(&node_name) {
        return Err(ServerError::NodeIsOpen);
    }

    unimplemented!();

    /*

    let remove_res = server_state.store.remove_node(node_name.clone()).await;
    match remove_res {
        Ok(()) => unimplemented!(),
        Err(e) => {
            warn!("handle_remove_node: store error: {:?}", e);
            user_sender.send(ServerToUser::ResponseRemoveNode(node_name))
                .await
                .map_err(|_| ServerError::UserSenderError)?;
        },
    }
    Ok(())
    */
}

#[allow(unused)]
pub async fn handle_user_to_server<S,ST,CG,US>(
    user_to_server: UserToServer, 
    server_state: &mut ServerState<ST>,
    compact_gen: &mut CG,
    user_sender: &mut US,
    _spawner: &S) -> Result<(), ServerError> 
where
    S: Spawn,
    ST: Store,
    CG: GenPrivateKey,
    US: Sink<ServerToUser> + Unpin,
{
    match user_to_server {
        UserToServer::RequestCreateNode(request_create_node) => handle_create_node(request_create_node, server_state, compact_gen, user_sender).await?,
        UserToServer::RequestRemoveNode(node_name) => handle_remove_node(node_name, server_state, user_sender).await?,
        UserToServer::RequestOpenNode(_node_name) => unimplemented!(),
        UserToServer::RequestCloseNode(_node_id) => unimplemented!(),
        UserToServer::Node(node_id, user_to_compact) => {
            let node_state = if let Some(node_state) = server_state.open_nodes.get_mut(&node_id) {
                node_state
            } else {
                warn!("UserToServer::Node: nonexistent node {:?}", node_id);
                return Ok(());
            };
            // TODO: 
            // - Generate a random uid here.
            // - Remember that the uid is related to a certain id provided by the user.
            let user_to_compact_ack = unimplemented!();
            node_state.sender.send(user_to_compact_ack).await.map_err(|_| ServerError::NodeSenderError)?;
        },
    }
    Ok(())
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
                unimplemented!();
                /*
                let (node_id, compact_to_user_ack) = compact_node_event;

                // TODO: Find the matching user's request id.
                if server_state.open_nodes.contains_key(&node_id) {
                    user_sender.send(ServerToUser::Node(node_id, compact_to_user_ack.inner))
                        .await
                        .map_err(|_| ServerError::UserSenderError)?;
                } else {
                    // The user was told that this node is closed,
                    // so we do not want to forward any more messages from this node to the user.
                    warn!("inner_server_loop: compact_node_event from a node {:?} that is closed.", node_id);
                }
                */
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
