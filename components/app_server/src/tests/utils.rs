use std::collections::HashMap;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, TryFutureExt};

use proto::crypto::PublicKey;

use proto::app_server::messages::{NamedRelayAddress, NodeReport};
use proto::funder::messages::{FunderIncomingControl, FunderOutgoingControl};
use proto::index_client::messages::{
    AppServerToIndexClient, IndexClientReport, IndexClientToAppServer,
};
use proto::index_server::messages::NamedIndexServerAddress;
use proto::report::messages::FunderReport;

use crate::server::{app_server_loop, IncomingAppConnection};

/// A helper function to quickly create a dummy NamedRelayAddress.
pub fn dummy_named_relay_address(index: u8) -> NamedRelayAddress<u32> {
    NamedRelayAddress {
        public_key: PublicKey::from(&[index; PublicKey::len()]),
        address: index as u32,
        name: format!("relay-{}", index),
    }
}

/*
/// A helper function to quickly create a dummy RelayAddress.
pub fn dummy_relay_address(index: u8) -> RelayAddress<u32> {
    dummy_named_relay_address(index).into()
}
*/

/// A test util function.
/// Spawns an app server loop and returns all relevant channels
/// used for control or communication.
pub fn spawn_dummy_app_server<S>(
    spawner: S,
) -> (
    mpsc::Sender<FunderOutgoingControl<u32>>,
    mpsc::Receiver<FunderIncomingControl<u32>>,
    mpsc::Sender<IndexClientToAppServer<u32>>,
    mpsc::Receiver<AppServerToIndexClient<u32>>,
    mpsc::Sender<IncomingAppConnection<u32>>,
    NodeReport<u32>,
)
where
    S: Spawn + Clone + Send + 'static,
{
    let (funder_sender, from_funder) = mpsc::channel(0);
    let (to_funder, funder_receiver) = mpsc::channel(0);

    let (index_client_sender, from_index_client) = mpsc::channel(0);
    let (to_index_client, index_client_receiver) = mpsc::channel(0);

    let (connections_sender, incoming_connections) = mpsc::channel(0);

    // Create a dummy initial_node_report:
    let funder_report = FunderReport {
        local_public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
        relays: vec![dummy_named_relay_address(0), dummy_named_relay_address(1)]
            .into_iter()
            .collect(),
        friends: HashMap::new(),
    };

    let server100 = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
        address: 100u32,
        name: "server100".to_owned(),
    };

    let server101 = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xbb; PublicKey::len()]),
        address: 101u32,
        name: "server101".to_owned(),
    };

    let index_client_report = IndexClientReport {
        index_servers: vec![server100, server101],
        opt_connected_server: Some(PublicKey::from(&[0xaa; PublicKey::len()])),
    };

    let initial_node_report = NodeReport {
        funder_report,
        index_client_report,
    };

    let fut_loop = app_server_loop(
        from_funder,
        to_funder,
        from_index_client,
        to_index_client,
        incoming_connections,
        initial_node_report.clone(),
        spawner.clone(),
    )
    .map_err(|e| error!("app_server_loop() error: {:?}", e))
    .map(|_| ());

    spawner.spawn(fut_loop).unwrap();

    (
        funder_sender,
        funder_receiver,
        index_client_sender,
        index_client_receiver,
        connections_sender,
        initial_node_report,
    )
}
