use futures::channel::oneshot;
use futures::task::Spawn;
use futures::{FutureExt, select};

use crypto::identity::PublicKey;
use timer::TimerClient;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_connector::ClientConnector;

async fn connect<A,C,S>(connector: C,
                address: A, 
                public_key: PublicKey, 
                keepalive_ticks: usize,
                timer_client: TimerClient,
                closer: oneshot::Receiver<()>,
                spawner: S) -> Option<ConnPair<Vec<u8>, Vec<u8>>>
where
    A: Sync + Send + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    S: Spawn + Clone + Sync + Send,
{
    let mut client_connector = ClientConnector::new(connector, spawner, timer_client, keepalive_ticks);
    let connect_fut = client_connector.connect((address, public_key));

    select! {
        opt_conn_pair = connect_fut.fuse() => opt_conn_pair,
        _closer = closer.fuse() => None,
    }
}
