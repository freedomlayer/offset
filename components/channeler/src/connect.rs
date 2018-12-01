use futures::channel::oneshot;
use futures::task::Spawn;
use futures::{FutureExt, select};

use crypto::identity::PublicKey;
use timer::TimerClient;
use timer::utils::sleep_ticks;

use relay::client::connector::{Connector, ConnPair};
use relay::client::client_connector::ClientConnector;

pub enum ConnectError {
    Canceled,
    SleepTicksError,
}

enum ConnectSelect {
    Canceled,
    ConnectFailed,
    Connected(ConnPair<Vec<u8>,Vec<u8>>),
}

pub async fn connect<A,C,S>(connector: C,
                address: A, 
                public_key: PublicKey, 
                keepalive_ticks: usize,
                backoff_ticks: usize,
                timer_client: TimerClient,
                closer: oneshot::Receiver<()>,
                spawner: S) -> Result<ConnPair<Vec<u8>, Vec<u8>>, ConnectError>
where
    A: Sync + Send + Clone + 'static,
    C: Connector<Address=A, SendItem=Vec<u8>, RecvItem=Vec<u8>> + Clone + Send + Sync + 'static,
    S: Spawn + Clone + Sync + Send,
{
    let mut closer = closer.map(|_| ConnectSelect::Canceled).fuse();

    loop {
        let mut client_connector = ClientConnector::new(connector.clone(), spawner.clone(), timer_client.clone(), keepalive_ticks);
        let c_timer_client = timer_client.clone();
        let c_address = address.clone();
        let c_public_key = public_key.clone();
        let mut connect_fut = Box::pinned(async move { 
            Ok(match await!(client_connector.connect((c_address, c_public_key))) {
                Some(conn_pair) => ConnectSelect::Connected(conn_pair),
                None => {
                    await!(sleep_ticks(backoff_ticks, c_timer_client))
                        .map_err(|_| ConnectError::SleepTicksError)?;
                    ConnectSelect::ConnectFailed
                }
            })
        }).fuse();

        let select_res = select! {
            connect_fut = connect_fut => connect_fut?,
            closer = closer => closer,
        };

        match select_res {
            ConnectSelect::Canceled => return Err(ConnectError::Canceled),
            ConnectSelect::ConnectFailed => {}, // In this case we try again
            ConnectSelect::Connected(conn_pair) => return Ok(conn_pair),
        };
    }
}
