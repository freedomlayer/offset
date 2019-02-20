use std::collections::HashMap;

use futures::channel::{mpsc, oneshot};
use futures::{StreamExt, SinkExt};
use futures::task::{Spawn, SpawnExt};

use common::conn::{ConnPairVec, FutTransform, BoxFuture};
use proto::net::messages::NetAddress;

pub enum TestNetworkRequest {
    Listen((NetAddress, oneshot::Sender<mpsc::Receiver<ConnPairVec>>)),
    Connect((NetAddress, oneshot::Sender<ConnPairVec>)),
}

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
    pub fn new(sender: mpsc::Sender<TestNetworkRequest>) -> Self {
        TestNetworkClient {
            sender,
        }
    }

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

#[allow(unused)]
/// A test util, simulating a network.
/// Allows clients to listen on certain addresses and try to connect to certain addresses.
/// No two listeners can listen on the same address.
pub fn create_test_network<S>(spawner: &mut S) -> TestNetworkClient
where
    S: Spawn,
{

    let (request_sender, incoming_requests) = mpsc::channel(0);
    spawner.spawn(test_network_loop(incoming_requests)).unwrap();

    TestNetworkClient::new(request_sender)
}


#[cfg(test)]
mod tests {
    use super::*;

    use std::convert::TryFrom;

    use futures::executor::ThreadPool;

    /// A helper function to create a net_address from a &str:
    fn net_address(from: &str) -> NetAddress {
        NetAddress::try_from(from.to_string()).unwrap()
    }

    async fn task_test_network_basic<S>(mut spawner: S) 
    where
        S: Spawn,
    {

        let mut net_client1 = create_test_network(&mut spawner);
        let mut net_client2 = net_client1.clone();
        let mut net_client3 = net_client1.clone();

        let mut incoming1 = await!(net_client1.listen(net_address("net_client1"))).unwrap();
        let mut incoming2 = await!(net_client2.listen(net_address("net_client2"))).unwrap();


        for _ in 0 .. 3 {
            let (mut sender3, mut receiver3) = await!(net_client3.transform(net_address("net_client1"))).unwrap();
            let (mut sender1, mut receiver1) = await!(incoming1.next()).unwrap();

            await!(sender1.send(vec![1,2,3])).unwrap();
            assert_eq!(await!(receiver3.next()), Some(vec![1,2,3]));

            await!(sender3.send(vec![3,2,1])).unwrap();
            assert_eq!(await!(receiver1.next()), Some(vec![3,2,1]));
        }

        for _ in 0 .. 3 {
            let (mut sender3, mut receiver3) = await!(net_client3.transform(net_address("net_client2"))).unwrap();
            let (mut sender2, mut receiver2) = await!(incoming2.next()).unwrap();

            await!(sender2.send(vec![1,2,3])).unwrap();
            assert_eq!(await!(receiver3.next()), Some(vec![1,2,3]));

            await!(sender3.send(vec![3,2,1])).unwrap();
            assert_eq!(await!(receiver2.next()), Some(vec![3,2,1]));
        }

        drop(incoming2);

        assert!(await!(net_client3.transform(net_address("net_client2"))).is_none());
    }

    #[test]
    fn test_test_network_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_test_network_basic(thread_pool.clone()));
    }
}
