use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{select, Future, FutureExt, SinkExt, Stream, StreamExt};

use derive_more::*;

use common::conn::{ConnPairVec, FutTransform, FuncFutTransform};

use crypto::rand::CryptoRandom;
use proto::crypto::PublicKey;

use database::DatabaseClient;
use identity::IdentityClient;
use timer::TimerClient;

use app_server::{app_server_loop, AppServerError, IncomingAppConnection};
use channeler::{spawn_channeler, ChannelerError};
use funder::types::{
    ChannelerConfig, FunderIncomingComm, FunderOutgoingComm, IncomingLivenessMessage,
};
use funder::{funder_loop, FunderError, FunderState};
use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;

use index_client::{spawn_index_client, IndexClientError};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    ChannelerToFunder, FriendMessage, FunderIncomingControl, FunderOutgoingControl,
    FunderToChanneler,
};
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer};
use proto::index_server::messages::IndexServerAddress;
use proto::net::messages::NetAddress;
use proto::report::convert::funder_report_to_index_client_state;

use crate::types::{create_node_report, NodeConfig, NodeMutation, NodeState};


#[derive(Debug, From)]
pub enum NodeError {
    RequestPublicKeyError,
    DatabaseIdentityMismatch,
    SpawnError,
    ChannelerError(ChannelerError),
    FunderError(FunderError),
    IndexClientError(IndexClientError),
    AppServerError(AppServerError),
}

fn node_spawn_channeler<C, R, S>(
    node_config: &NodeConfig,
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    timer_client: TimerClient,
    connector: C,
    rng: R,
    from_funder: mpsc::Receiver<FunderToChanneler<RelayAddress>>,
    to_funder: mpsc::Sender<ChannelerToFunder>,
    spawner: S,
) -> Result<impl Future<Output = Result<(), ChannelerError>>, NodeError>
where
    C: FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        node_config.ticks_to_rekey,
        spawner.clone(),
    );

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        node_config.keepalive_ticks,
        spawner.clone(),
    );

    let encrypt_keepalive = FuncFutTransform::new(move |(opt_public_key, conn_pair_vec)| {
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_keepalive_transform = keepalive_transform.clone();
        Box::pin(async move {
            let (public_key, conn_pair_vec) = c_encrypt_transform.transform((opt_public_key, conn_pair_vec)).await?;
            let conn_pair_vec = c_keepalive_transform.transform(conn_pair_vec).await;
            Some((public_key, conn_pair_vec))
        })
    });


    let enc_relay_connector = FuncFutTransform::new(move |relay_address: RelayAddress| {
        let mut c_connector = connector.clone();
        Box::pin(async move {
            c_connector.transform((relay_address.public_key, relay_address.address)).await
        })
    });

    spawner
        .spawn_with_handle(spawn_channeler(
            local_public_key,
            timer_client,
            node_config.backoff_ticks,
            node_config.conn_timeout_ticks,
            node_config.max_concurrent_encrypt,
            enc_relay_connector,
            encrypt_keepalive,
            from_funder,
            to_funder,
            spawner.clone(),
        ))
        .map_err(|_| NodeError::SpawnError)
}

fn node_spawn_funder<R, S>(
    node_config: &NodeConfig,
    identity_client: IdentityClient,
    funder_state: FunderState<NetAddress>,
    mut database_client: DatabaseClient<NodeMutation<NetAddress>>,
    mut from_channeler: mpsc::Receiver<ChannelerToFunder>,
    mut to_channeler: mpsc::Sender<FunderToChanneler<RelayAddress>>,
    from_app_server: mpsc::Receiver<FunderIncomingControl<NetAddress>>,
    to_app_server: mpsc::Sender<FunderOutgoingControl<NetAddress>>,
    rng: R,
    spawner: S,
) -> Result<impl Future<Output = Result<(), FunderError>>, NodeError>
where
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    // TODO: Should we give a length > 0 for this adapter's channel?
    let (request_sender, mut request_receiver) = mpsc::channel(0);
    let funder_db_client = DatabaseClient::new(request_sender);

    let database_adapter_fut = async move {
        while let Some(request) = request_receiver.next().await {
            let mutations = request
                .mutations
                .into_iter()
                .map(NodeMutation::Funder)
                .collect::<Vec<_>>();

            if let Err(e) = database_client.mutate(mutations).await {
                error!("error in funder database adapter: {:?}", e);
                return;
            }
            if let Err(e) = request.response_sender.send(()) {
                error!("error in funder database adapter: {:?}", e);
                return;
            }
        }
    };
    spawner
        .spawn(database_adapter_fut)
        .map_err(|_| NodeError::SpawnError)?;

    // Channeler to funder adapter:
    let (mut incoming_comm_sender, incoming_comm) = mpsc::channel(0);
    let channeler_to_funder_adapter = async move {
        while let Some(channeler_message) = from_channeler.next().await {
            let opt_to_funder_message = match channeler_message {
                ChannelerToFunder::Online(public_key) => Some(FunderIncomingComm::Liveness(
                    IncomingLivenessMessage::Online(public_key),
                )),
                ChannelerToFunder::Offline(public_key) => Some(FunderIncomingComm::Liveness(
                    IncomingLivenessMessage::Offline(public_key),
                )),
                ChannelerToFunder::Message((public_key, data)) => {
                    if let Ok(friend_message) = FriendMessage::proto_deserialize(&data[..]) {
                        Some(FunderIncomingComm::Friend((public_key, friend_message)))
                    } else {
                        // We discard the message if we can't deserialize it:
                        None
                    }
                }
            };
            if let Some(to_funder_message) = opt_to_funder_message {
                if incoming_comm_sender.send(to_funder_message).await.is_err() {
                    return;
                }
            }
        }
    };

    spawner
        .spawn(channeler_to_funder_adapter)
        .map_err(|_| NodeError::SpawnError)?;

    let (outgoing_comm_sender, mut outgoing_comm) = mpsc::channel(0);

    // Funder to Channeler adapter:
    let funder_to_channeler_adapter = async move {
        while let Some(funder_message) = outgoing_comm.next().await {
            let to_channeler_message = match funder_message {
                FunderOutgoingComm::ChannelerConfig(channeler_config) => match channeler_config {
                    ChannelerConfig::SetRelays(relay_addresses) => {
                        FunderToChanneler::SetRelays(relay_addresses)
                    }
                    ChannelerConfig::UpdateFriend(channeler_update_friend) => {
                        FunderToChanneler::UpdateFriend(channeler_update_friend)
                    }
                    ChannelerConfig::RemoveFriend(friend_public_key) => {
                        FunderToChanneler::RemoveFriend(friend_public_key)
                    }
                },
                FunderOutgoingComm::FriendMessage((public_key, friend_message)) => {
                    // let data = serialize_friend_message(&friend_message);
                    let data = friend_message.proto_serialize();
                    FunderToChanneler::Message((public_key, data))
                }
            };
            if to_channeler.send(to_channeler_message).await.is_err() {
                return;
            }
        }
    };

    spawner
        .spawn(funder_to_channeler_adapter)
        .map_err(|_| NodeError::SpawnError)?;

    let funder_fut = funder_loop(
        identity_client.clone(),
        rng.clone(),
        from_app_server,
        incoming_comm,
        to_app_server,
        outgoing_comm_sender,
        node_config.max_node_relays,
        node_config.max_operations_in_batch,
        node_config.max_pending_user_requests,
        funder_state,
        funder_db_client,
    );

    spawner
        .spawn_with_handle(funder_fut)
        .map_err(|_| NodeError::SpawnError)
}

async fn node_spawn_index_client<C, R, S>(
    node_config: &NodeConfig,
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    timer_client: TimerClient,
    node_state: &NodeState<NetAddress>,
    mut database_client: DatabaseClient<NodeMutation<NetAddress>>,
    from_app_server: mpsc::Receiver<AppServerToIndexClient<NetAddress>>,
    to_app_server: mpsc::Sender<IndexClientToAppServer<NetAddress>>,
    connector: C,
    rng: R,
    spawner: S,
) -> Result<impl Future<Output = Result<(), IndexClientError>>, NodeError>
where
    C: FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let initial_node_report = create_node_report(&node_state);

    // Database adapter:
    let (request_sender, mut request_receiver) = mpsc::channel(0);
    let index_client_db_client = DatabaseClient::new(request_sender);

    let database_adapter_fut = async move {
        while let Some(request) = request_receiver.next().await {
            let mutations = request
                .mutations
                .into_iter()
                .map(NodeMutation::IndexClient)
                .collect::<Vec<_>>();

            if let Err(e) = database_client.mutate(mutations).await {
                error!("error in index_client database adapter: {:?}", e);
                return;
            }
            if let Err(e) = request.response_sender.send(()) {
                error!("error in index_client database adapter: {:?}", e);
                return;
            }
        }
    };
    spawner
        .spawn(database_adapter_fut)
        .map_err(|_| NodeError::SpawnError)?;

    let index_client_state =
        funder_report_to_index_client_state(&initial_node_report.funder_report);

    /*
    let encrypt_transform = SecureChannel::new(
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        node_config.ticks_to_rekey,
        spawner.clone(),
    );

    let keepalive_transform = KeepAliveChannel::new(
        timer_client.clone(),
        node_config.keepalive_ticks,
        spawner.clone(),
    );

    let enc_keepalive_connector = EncKeepaliveConnector::new(
        encrypt_transform.clone(),
        keepalive_transform.clone(),
        version_connector.clone(),
        spawner.clone(),
    );
    */

    let index_connector = FuncFutTransform::new(move |index_server_address: IndexServerAddress| {
        let mut c_connector = connector.clone();
        Box::pin(async move {
            c_connector.transform((index_server_address.public_key, index_server_address.address)).await
        })
    });

    spawn_index_client(
        local_public_key,
        node_state.index_client_config.clone(),
        index_client_state,
        identity_client,
        timer_client,
        index_client_db_client,
        from_app_server,
        to_app_server,
        node_config.max_open_index_client_requests,
        node_config.keepalive_ticks,
        node_config.backoff_ticks,
        index_connector,
        rng,
        spawner.clone(),
    )
    .await
    .map_err(|_| NodeError::SpawnError)
}

// TODO: Possibly rename this function?
pub async fn node<C, IA, R, S>(
    node_config: NodeConfig,
    identity_client: IdentityClient,
    timer_client: TimerClient,
    node_state: NodeState<NetAddress>,
    database_client: DatabaseClient<NodeMutation<NetAddress>>,
    connector: C,
    incoming_apps: IA,
    rng: R,
    spawner: S,
) -> Result<(), NodeError>
where
    C: FutTransform<Input = (PublicKey, NetAddress), Output = Option<ConnPairVec>> + Clone + Send + 'static,
    IA: Stream<Item = IncomingAppConnection<NetAddress>> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    // Get local public key:
    let local_public_key = identity_client
        .request_public_key()
        .await
        .map_err(|_| NodeError::RequestPublicKeyError)?;

    // Make sure that the local public key in the database
    // matches the local public key from the provided identity file:
    if node_state.funder_state.local_public_key != local_public_key {
        return Err(NodeError::DatabaseIdentityMismatch);
    }

    let initial_node_report = create_node_report(&node_state);

    // Channeler <--> Funder
    let (channeler_to_funder_sender, channeler_to_funder_receiver) =
        mpsc::channel(node_config.channel_len);
    let (funder_to_channeler_sender, funder_to_channeler_receiver) =
        mpsc::channel(node_config.channel_len);

    let channeler_handle = node_spawn_channeler(
        &node_config,
        local_public_key.clone(),
        identity_client.clone(),
        timer_client.clone(),
        connector.clone(),
        rng.clone(),
        funder_to_channeler_receiver,
        channeler_to_funder_sender,
        spawner.clone(),
    )?;

    // AppServer <--> Funder
    let (app_server_to_funder_sender, app_server_to_funder_receiver) =
        mpsc::channel(node_config.channel_len);
    let (funder_to_app_server_sender, funder_to_app_server_receiver) =
        mpsc::channel(node_config.channel_len);

    let funder_handle = node_spawn_funder(
        &node_config,
        identity_client.clone(),
        node_state.funder_state.clone(),
        database_client.clone(),
        channeler_to_funder_receiver,
        funder_to_channeler_sender,
        app_server_to_funder_receiver,
        funder_to_app_server_sender,
        rng.clone(),
        spawner.clone(),
    )?;

    // AppServer <--> IndexClient
    let (app_server_to_index_client_sender, app_server_to_index_client_receiver) =
        mpsc::channel(node_config.channel_len);
    let (index_client_to_app_server_sender, index_client_to_app_server_receiver) =
        mpsc::channel(node_config.channel_len);

    let app_server_fut = app_server_loop(
        funder_to_app_server_receiver,
        app_server_to_funder_sender,
        index_client_to_app_server_receiver,
        app_server_to_index_client_sender,
        incoming_apps,
        initial_node_report.clone(),
        spawner.clone(),
    );

    let app_server_handle = spawner
        .spawn_with_handle(app_server_fut)
        .map_err(|_| NodeError::SpawnError)?;

    let index_client_handle = node_spawn_index_client(
        &node_config,
        local_public_key,
        identity_client,
        timer_client,
        &node_state,
        database_client,
        app_server_to_index_client_receiver,
        index_client_to_app_server_sender,
        connector,
        rng,
        spawner,
    )
    .await?;

    // Wait for death of any component
    select! {
        res = channeler_handle.fuse() => res?,
        res = funder_handle.fuse() => res?,
        res = app_server_handle.fuse() => res?,
        res = index_client_handle.fuse() => res?,
    }
    Ok(())
}
