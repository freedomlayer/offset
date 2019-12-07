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
use crate::compact_node::{ToUser, FromUser};
use crate::gen::GenPrivateKey;
use crate::store::Store;

pub type ConnPairServer = ConnPair<ServerToUser, UserToServer>;

#[derive(Debug)]
pub enum ServerError {
    UserSenderError,
    NodeSenderError,
    DerivePublicKeyError,
}

type CompactNodeEvent = (NodeId, ToUser);

pub enum ServerEvent {
    User(UserToServer),
    UserClosed,
    CompactNode(CompactNodeEvent),
}

#[derive(Debug)]
pub struct OpenNode {
    pub node_name: NodeName,
    pub sender: mpsc::Sender<FromUser>,
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
}

#[allow(unused)]
pub async fn handle_create_node_remote<ST,US>(
    create_node_local: CreateNodeRemote, 
    store: &mut ST, 
    user_sender: &mut US) -> Result<(), ServerError> 
where
    ST: Store,
    US: Sink<ServerToUser> + Unpin,
{
    unimplemented!();
}

pub async fn handle_user_to_server<S,ST,CG,US>(
    user_to_server: UserToServer, 
    server_state: &mut ServerState<ST>,
    compact_gen: &mut CG,
    mut user_sender: US,
    _spawner: &S) -> Result<(), ServerError> 
where
    S: Spawn,
    ST: Store,
    CG: GenPrivateKey,
    US: Sink<ServerToUser> + Unpin,
{
    match user_to_server {
        UserToServer::RequestCreateNode(request_create_node) => {
            match request_create_node {
                RequestCreateNode::CreateNodeLocal(local) => 
                    handle_create_node_local(local, &mut server_state.store, &mut user_sender, compact_gen).await?,
                RequestCreateNode::CreateNodeRemote(remote) => 
                    handle_create_node_remote(remote, &mut server_state.store, &mut user_sender).await?,
            }
        },
        UserToServer::RequestRemoveNode(_node_name) => unimplemented!(),
        UserToServer::RequestOpenNode(_node_name) => unimplemented!(),
        UserToServer::RequestCloseNode(_node_id) => unimplemented!(),
        UserToServer::Node(node_id, from_user) => {
            let node_state = if let Some(node_state) = server_state.open_nodes.get_mut(&node_id) {
                node_state
            } else {
                warn!("UserToServer::Node: nonexistent node {:?}", node_id);
                return Ok(());
            };
            node_state.sender.send(from_user).await.map_err(|_| ServerError::NodeSenderError)?;
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
                let (node_id, to_user) = compact_node_event;
                if server_state.open_nodes.contains_key(&node_id) {
                    user_sender.send(ServerToUser::Node(node_id, to_user))
                        .await
                        .map_err(|_| ServerError::UserSenderError)?;
                } else {
                    // The user was told that this node is closed,
                    // so we do not want to forward any more messages from this node to the user.
                    warn!("inner_server_loop: compact_node_event from a node {:?} that is closed.", node_id);
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
