use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, TryFutureExt};

use im::hashmap::HashMap as ImHashMap;

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl};
use proto::report::messages::FunderReport;
use proto::app_server::messages::NodeReport;
use proto::index_client::messages::{IndexClientToAppServer, 
    AppServerToIndexClient, IndexClientReport};
use proto::index_server::messages::NamedIndexServerAddress;

use crate::server::{IncomingAppConnection, app_server_loop};

use proto::funder::scheme::FunderScheme;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestFunderScheme;

impl FunderScheme for TestFunderScheme {
    /// An anonymous address
    type Address = Vec<u32>;
    /// An address that contains a name (Provided by the user of this node)
    type NamedAddress = Vec<(String, u32)>;

    /// A function to convert a NamedAddress to Address
    fn anonymize_address(vec: Self::NamedAddress) -> Self::Address {
        vec
            .into_iter()
            .map(|(_name, num)| num)
            .collect::<Vec<_>>()
    }
}

/// A test util function.
/// Spawns an app server loop and returns all relevant channels
/// used for control or communication.
pub fn spawn_dummy_app_server<S>(mut spawner: S) -> 
    (mpsc::Sender<FunderOutgoingControl<TestFunderScheme>>,
     mpsc::Receiver<FunderIncomingControl<TestFunderScheme>>,
     mpsc::Sender<IndexClientToAppServer<u64>>,
     mpsc::Receiver<AppServerToIndexClient<u64>>,
     mpsc::Sender<IncomingAppConnection<TestFunderScheme,u64>>,
     NodeReport<TestFunderScheme,u64>)

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
        address: vec![("0".to_string(), 0u32), ("1".to_string(), 1u32)],
        friends: ImHashMap::new(),
        num_ready_receipts: 0,
    };

    let server100 = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]), 
        address: 100u64,
        name: "server100".to_owned(),
    };

    let server101 = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]), 
        address: 101u64,
        name: "server101".to_owned(),
    };

    let index_client_report = IndexClientReport {
        index_servers: vec![server100, server101],
        opt_connected_server: Some(PublicKey::from(&[0xaa; PUBLIC_KEY_LEN])),
    };

    let initial_node_report = NodeReport {
        funder_report,
        index_client_report,
    };

    let max_node_relays = 16;
    let fut_loop = app_server_loop(from_funder,
                    to_funder,
                    from_index_client,
                    to_index_client,
                    incoming_connections,
                    initial_node_report.clone(),
                    max_node_relays,
                    spawner.clone())
        .map_err(|e| error!("app_server_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(fut_loop).unwrap();

    (funder_sender, funder_receiver,
     index_client_sender, index_client_receiver,
     connections_sender, initial_node_report)
}

