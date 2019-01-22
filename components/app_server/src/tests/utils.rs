use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, TryFutureExt};

use im::hashmap::HashMap as ImHashMap;

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl};
use proto::funder::report::FunderReport;
use proto::app_server::messages::NodeReport;
use proto::index_client::messages::{IndexClientToAppServer, 
    AppServerToIndexClient, IndexClientReport};

use crate::server::{IncomingAppConnection, app_server_loop};

/// A test util function.
/// Spawns an app server loop and returns all relevant channels
/// used for control or communication.
pub fn spawn_dummy_app_server<S>(mut spawner: S) -> 
    (mpsc::Sender<FunderOutgoingControl<Vec<u32>>>,
     mpsc::Receiver<FunderIncomingControl<Vec<u32>>>,
     mpsc::Sender<IndexClientToAppServer<u64>>,
     mpsc::Receiver<AppServerToIndexClient<u64>>,
     mpsc::Sender<IncomingAppConnection<u32,u64>>,
     NodeReport<u32,u64>)

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
        local_public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
        address: vec![0u32, 1u32],
        friends: ImHashMap::new(),
        num_ready_receipts: 0,
    };

    let index_client_report = IndexClientReport {
        index_servers: vec![100u64, 101u64],
        opt_connected_server: Some(101u64),
    };

    let initial_node_report = NodeReport {
        funder_report,
        index_client_report,
    };

    let fut_loop = app_server_loop(from_funder,
                    to_funder,
                    from_index_client,
                    to_index_client,
                    incoming_connections,
                    initial_node_report.clone(),
                    spawner.clone())
        .map_err(|e| error!("app_server_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(fut_loop).unwrap();

    (funder_sender, funder_receiver,
     index_client_sender, index_client_receiver,
     connections_sender, initial_node_report)
}

