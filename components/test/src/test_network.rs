use std::collections::HashMap;

use futures::channel::{mpsc, oneshot};
use futures::{StreamExt, SinkExt};

use common::conn::{ConnPairVec, FutTransform, BoxFuture};
use proto::net::messages::NetAddress;

pub enum TestNetworkRequest {
    Listen((NetAddress, oneshot::Sender<mpsc::Receiver<ConnPairVec>>)),
    Connect((NetAddress, oneshot::Sender<ConnPairVec>)),
}

#[allow(unused)]
pub async fn test_network_loop(mut incoming_requests: mpsc::Receiver<TestNetworkRequest>) {

    let mut listeners: HashMap<NetAddress, mpsc::Sender<ConnPairVec>> = HashMap::new();

    while let Some(request) = await!(incoming_requests.next()) {
        match request {
            TestNetworkRequest::Listen((listen_address, receiver_sender)) => {
                if listeners.contains_key(&listen_address) {
                    // Someone is already listening on this address
                    continue;
                }
                let (conn_sender, conn_receiver) = mpsc::channel(0);
                if let Ok(_) = receiver_sender.send(conn_receiver) {
                    listeners.insert(listen_address, conn_sender);
                }
            },
            TestNetworkRequest::Connect((connect_address, oneshot_sender)) => {
                if let Some(mut conn_sender) = listeners.remove(&connect_address) {
                    let (connect_sender, listen_receiver) = mpsc::channel(0);
                    let (listen_sender, connect_receiver) = mpsc::channel(0);

                    if let Err(_) = await!(conn_sender.send((listen_sender, listen_receiver))) {
                        // Note that we dropped the listener's sender.
                        continue;
                    }

                    // Put the listener sender back in to the map:
                    listeners.insert(connect_address, conn_sender);
                    let _ = oneshot_sender.send((connect_sender, connect_receiver));
                }
            },
        }
    }
}

#[derive(Debug)]
pub enum TestNetworkClientError {
    ListenError,
}

pub struct TestNetworkClient {
    sender: mpsc::Sender<TestNetworkRequest>,
}

impl TestNetworkClient {
    #[allow(unused)]
    pub async fn listen(&mut self, net_address: NetAddress) 
        -> Result<mpsc::Receiver<ConnPairVec>, TestNetworkClientError> {

        let (response_sender, response_receiver) = oneshot::channel();
        await!(self.sender.send(TestNetworkRequest::Listen((net_address, response_sender))))
            .map_err(|_| TestNetworkClientError::ListenError)?;
        await!(response_receiver)
            .map_err(|_| TestNetworkClientError::ListenError)
    }
}

impl Clone for TestNetworkClient {
    fn clone(&self) -> Self {
        TestNetworkClient {
            sender: self.sender.clone(),
        }
    }
}

impl FutTransform for TestNetworkClient {
    type Input = NetAddress;
    type Output = Option<ConnPairVec>;

    #[allow(unused)]
    fn transform(&mut self, net_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        let (response_sender, response_receiver) = oneshot::channel();
        Box::pin(async move {
            await!(self.sender.send(TestNetworkRequest::Connect((net_address, response_sender))))
                .ok()?;
            await!(response_receiver).ok()
        })
    }
}

