use std::collections::HashMap;
use std::convert::TryFrom;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{SinkExt, StreamExt};

use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use proto::net::messages::NetAddress;

/// Length of a connection channel.
/// We might get a deadlock if this value is too small?
const CHANNEL_SIZE: usize = 0x100;

/// A helper function to create a net_address from a &str:
pub fn net_address(from: &str) -> NetAddress {
    NetAddress::try_from(from.to_string()).unwrap()
}

#[derive(Debug)]
pub enum SimNetworkRequest {
    Listen((NetAddress, oneshot::Sender<mpsc::Receiver<ConnPairVec>>)),
    Connect((NetAddress, oneshot::Sender<ConnPairVec>)),
}

pub async fn sim_network_loop(mut incoming_requests: mpsc::Receiver<SimNetworkRequest>) {
    let mut listeners: HashMap<NetAddress, mpsc::Sender<ConnPairVec>> = HashMap::new();

    while let Some(request) = incoming_requests.next().await {
        match request {
            SimNetworkRequest::Listen((listen_address, receiver_sender)) => {
                info!("SimNetworkRequest::Listen({:?})", listen_address);
                if let Some(cur_sender) = listeners.remove(&listen_address) {
                    if !cur_sender.is_closed() {
                        // Someone is already listening on this address
                        warn!(
                            "SimNetworkRequest::Listen: Listen address: {:?} is in use",
                            listen_address
                        );
                        listeners.insert(listen_address, cur_sender);
                        continue;
                    }
                }

                let (conn_sender, conn_receiver) = mpsc::channel(CHANNEL_SIZE);
                if let Ok(_) = receiver_sender.send(conn_receiver) {
                    listeners.insert(listen_address, conn_sender);
                } else {
                    warn!("SimNetworkRequest::Listen: Request failed");
                }
            }
            SimNetworkRequest::Connect((connect_address, oneshot_sender)) => {
                info!("SimNetworkRequest::Connect({:?})", connect_address);
                if let Some(mut conn_sender) = listeners.remove(&connect_address) {
                    let (connect_sender, listen_receiver) = mpsc::channel(CHANNEL_SIZE);
                    let (listen_sender, connect_receiver) = mpsc::channel(CHANNEL_SIZE);

                    if let Err(_) = conn_sender
                        .send(ConnPairVec::from_raw(listen_sender, listen_receiver))
                        .await
                    {
                        // Note that we dropped the listener's sender.
                        warn!("SimNetworkRequest::Connect: Connection request failed");
                        continue;
                    }

                    // Put the listener sender back in to the map:
                    listeners.insert(connect_address, conn_sender);
                    if let Err(_) =
                        oneshot_sender.send(ConnPairVec::from_raw(connect_sender, connect_receiver))
                    {
                        warn!("SimNetworkRequest::Connect: Failure sending pair!");
                    }
                } else {
                    warn!("Connection failed: No listeners at: {:?}", connect_address);
                }
            }
        }
    }
    info!("sim_network_loop() closed");
}

#[derive(Debug)]
pub enum SimNetworkClientError {
    SendRequestError,
    ReceiveResponseError,
}

#[derive(Clone)]
pub struct SimNetworkClient {
    sender: mpsc::Sender<SimNetworkRequest>,
}

impl SimNetworkClient {
    pub fn new(sender: mpsc::Sender<SimNetworkRequest>) -> Self {
        SimNetworkClient { sender }
    }

    pub async fn listen(
        &mut self,
        net_address: NetAddress,
    ) -> Result<mpsc::Receiver<ConnPairVec>, SimNetworkClientError> {
        let (response_sender, response_receiver) = oneshot::channel();
        self.sender
            .send(SimNetworkRequest::Listen((net_address, response_sender)))
            .await
            .map_err(|_| SimNetworkClientError::SendRequestError)?;
        response_receiver
            .await
            .map_err(|_| SimNetworkClientError::ReceiveResponseError)
    }
}

impl FutTransform for SimNetworkClient {
    type Input = NetAddress;
    type Output = Option<ConnPairVec>;

    #[allow(unused)]
    fn transform(&mut self, net_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        let (response_sender, response_receiver) = oneshot::channel();
        Box::pin(async move {
            self.sender
                .send(SimNetworkRequest::Connect((net_address, response_sender)))
                .await
                .ok()?;
            response_receiver.await.ok()
        })
    }
}

#[allow(unused)]
/// A test util, simulating a network.
/// Allows clients to listen on certain addresses and try to connect to certain addresses.
/// No two listeners can listen on the same address.
pub fn create_sim_network<S>(spawner: &mut S) -> SimNetworkClient
where
    S: Spawn,
{
    let (request_sender, incoming_requests) = mpsc::channel(CHANNEL_SIZE);
    spawner.spawn(sim_network_loop(incoming_requests)).unwrap();

    SimNetworkClient::new(request_sender)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{block_on, ThreadPool};

    async fn task_sim_network_basic<S>(mut spawner: S)
    where
        S: Spawn,
    {
        let mut net_client1 = create_sim_network(&mut spawner);
        let mut net_client2 = net_client1.clone();
        let mut net_client3 = net_client1.clone();

        let mut incoming1 = net_client1
            .listen(net_address("net_client1"))
            .await
            .unwrap();
        let mut incoming2 = net_client2
            .listen(net_address("net_client2"))
            .await
            .unwrap();

        for _ in 0..3 {
            let (mut sender3, mut receiver3) = net_client3
                .transform(net_address("net_client1"))
                .await
                .unwrap()
                .split();
            let (mut sender1, mut receiver1) = incoming1.next().await.unwrap().split();

            sender1.send(vec![1, 2, 3]).await.unwrap();
            assert_eq!(receiver3.next().await, Some(vec![1, 2, 3]));

            sender3.send(vec![3, 2, 1]).await.unwrap();
            assert_eq!(receiver1.next().await, Some(vec![3, 2, 1]));
        }

        for _ in 0..3 {
            let (mut sender3, mut receiver3) = net_client3
                .transform(net_address("net_client2"))
                .await
                .unwrap()
                .split();
            let (mut sender2, mut receiver2) = incoming2.next().await.unwrap().split();

            sender2.send(vec![1, 2, 3]).await.unwrap();
            assert_eq!(receiver3.next().await, Some(vec![1, 2, 3]));

            sender3.send(vec![3, 2, 1]).await.unwrap();
            assert_eq!(receiver2.next().await, Some(vec![3, 2, 1]));
        }

        drop(incoming2);

        assert!(net_client3
            .transform(net_address("net_client2"))
            .await
            .is_none());
    }

    #[test]
    fn test_sim_network_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_sim_network_basic(thread_pool.clone()));
    }
}
