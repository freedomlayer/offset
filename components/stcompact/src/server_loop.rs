use std::collections::HashMap;

use futures::{stream, StreamExt, SinkExt, channel::mpsc, future};
use futures::task::Spawn;

use common::select_streams::select_streams;
use common::conn::{ConnPair, BoxStream};

use crate::messages::{ServerToUser, UserToServer, NodeId, NodeName};
use crate::compact_node::{ToUser, FromUser};

pub type ConnPairServer = ConnPair<ServerToUser, UserToServer>;

#[derive(Debug)]
pub enum ServerError {
    UserSenderError,
    NodeSenderError,
}

type CompactNodeEvent = (NodeId, ToUser);

pub enum ServerEvent {
    User(UserToServer),
    UserClosed,
    CompactNode(CompactNodeEvent),
}

#[derive(Debug)]
pub struct NodeState {
    pub node_name: NodeName,
    pub sender: mpsc::Sender<FromUser>,
    // TODO: How to signal to close the node?
    // Possibly await some handle (If we use spawn_with_handle for example?)
}

#[derive(Debug)]
pub struct ServerState {
    pub next_node_id: NodeId,
    pub nodes: HashMap<NodeId, NodeState>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            next_node_id: NodeId(0),
            nodes: HashMap::new(),
        }
    }
}

pub async fn handle_user_to_server<S>(
    user_to_server: UserToServer, 
    server_state: &mut ServerState,
    _spawner: &S) -> Result<(), ServerError> 
where
    S: Spawn,
{

    match user_to_server {
        UserToServer::RequestCreateNode(_request_create_node) => unimplemented!(),
        UserToServer::RequestRemoveNode(_node_name) => unimplemented!(),
        UserToServer::RequestOpenNode(_node_name) => unimplemented!(),
        UserToServer::RequestCloseNode(_node_id) => unimplemented!(),
        UserToServer::Node(node_id, from_user) => {
            let node_state = if let Some(node_state) = server_state.nodes.get_mut(&node_id) {
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
pub async fn inner_server_loop<S>(
    conn_pair: ConnPairServer,
    spawner: S,
    mut opt_event_sender: Option<mpsc::Sender<()>>) -> Result<(), ServerError> 
where
    S: Spawn,
{

    let mut server_state = ServerState::new();

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
                handle_user_to_server(user_to_server, &mut server_state, &spawner).await?;
            }
            ServerEvent::UserClosed => return Ok(()),
            ServerEvent::CompactNode(compact_node_event) => {
                let (node_id, to_user) = compact_node_event;
                if server_state.nodes.contains_key(&node_id) {
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
