use futures::task::{Spawn, SpawnExt};
use futures::executor::ThreadPool;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, TryFutureExt, StreamExt, SinkExt};

use common::dummy_connector::{DummyConnector, ConnRequest};

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::uid::{Uid, UID_LEN};
use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexClientReportMutation, UpdateFriend,
                                    IndexMutation, RequestRoutes,
                                    ResponseRoutesResult};

use crate::index_client::{index_client_loop, IndexClientConfig,
                        IndexClientConfigMutation};
use crate::seq_friends::{SeqFriendsClient, SeqFriendsRequest};
use crate::single_client::{SingleClientControl, SingleClientError};
use crate::client_session::SessionHandle;


/// A test util struct
/// Holds sender/receiver interface for an IndexClient.
struct IndexClientControl<ISA> {
    app_server_sender: mpsc::Sender<AppServerToIndexClient<ISA>>,
    app_server_receiver: mpsc::Receiver<IndexClientToAppServer<ISA>>,
    seq_friends_receiver: mpsc::Receiver<SeqFriendsRequest>,
    session_receiver: mpsc::Receiver<ConnRequest<ISA,Option<SessionHandle>>>,
    database_receiver: mpsc::Receiver<ConnRequest<IndexClientConfigMutation<ISA>,Option<()>>>,
    tick_sender: mpsc::Sender<()>,
    #[allow(unused)]
    max_open_requests: usize,
    #[allow(unused)]
    keepalive_ticks: usize,
    backoff_ticks: usize,
}

/// Create a basic IndexClientControl, used for testing
fn basic_index_client<S>(mut spawner: S) -> IndexClientControl<u32> 
where
    S: Spawn + Clone + Send + 'static,
{
    let (app_server_sender, from_app_server) = mpsc::channel(0);
    let (to_app_server, app_server_receiver) = mpsc::channel(0);

    let index_client_config = IndexClientConfig {
        index_servers: vec![0x1337u32],
    };

    let (seq_friends_sender, seq_friends_receiver) = mpsc::channel(0);
    let seq_friends_client = SeqFriendsClient::new(seq_friends_sender);

    let (session_sender, session_receiver) = mpsc::channel(0);
    let index_client_session = DummyConnector::new(session_sender);

    let (database_sender, database_receiver) = mpsc::channel(0);
    let database = DummyConnector::new(database_sender);

    let max_open_requests = 2;
    let keepalive_ticks = 8;
    let backoff_ticks = 4;

    let (tick_sender, timer_stream) = mpsc::channel::<()>(0);

    let loop_fut = index_client_loop(from_app_server,
                               to_app_server,
                               index_client_config,
                               seq_friends_client,
                               index_client_session,
                               max_open_requests,
                               keepalive_ticks,
                               backoff_ticks,
                               database,
                               timer_stream,
                               spawner.clone())
        .map_err(|e| error!("index_client_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut).unwrap();

    IndexClientControl {
        app_server_sender,
        app_server_receiver,
        seq_friends_receiver,
        session_receiver,
        database_receiver,
        tick_sender,
        max_open_requests,
        keepalive_ticks,
        backoff_ticks,
    }
}


impl<ISA> IndexClientControl<ISA> 
where
    ISA: std::cmp::Eq + std::fmt::Debug + Clone,
{
    /// Expect an IndexClient report of SetConnectedServer
    async fn expect_set_connected_server(&mut self, expected_opt_address: Option<ISA>) {
        match await!(self.app_server_receiver.next()).unwrap() {
            IndexClientToAppServer::ReportMutations(mut mutations) => {
                assert_eq!(mutations.len(), 1);
                match mutations.pop().unwrap() {
                    IndexClientReportMutation::SetConnectedServer(opt_address) =>
                        assert_eq!(opt_address, expected_opt_address),
                    _ => unreachable!(),
                };
            },
            _ => unreachable!(),
        };
    }

    /// Expect a connection to index server of address `server_address`.
    async fn expect_server_connection(&mut self, server_address: ISA) -> 
        (mpsc::Receiver<SingleClientControl>, oneshot::Sender<Result<(), SingleClientError>>) {

        // Wait for a connection request:
        let session_conn_request = await!(self.session_receiver.next()).unwrap();
        assert_eq!(session_conn_request.address, server_address);

        // Send a SessionHandle back to the index client:
        let (control_sender, mut control_receiver) = mpsc::channel(0);
        let (close_sender, close_receiver) = oneshot::channel();
        session_conn_request.reply(Some((control_sender, close_receiver)));

        // We should be notified that a connection to a server was established:
        await!(self.expect_set_connected_server(Some(server_address)));

        match await!(self.seq_friends_receiver.next()).unwrap() {
            SeqFriendsRequest::ResetCountdown(response_sender) => {
                response_sender.send(()).unwrap();
            },
            _ => unreachable!(),
        };

        // Feeding the send_full_state() task with one friend state:
        match await!(self.seq_friends_receiver.next()).unwrap() {
            SeqFriendsRequest::NextUpdate(response_sender) => {
                let update_friend = UpdateFriend {
                    public_key: PublicKey::from(PublicKey::from(&[0xaa; PUBLIC_KEY_LEN])),
                    send_capacity: 100,
                    recv_capacity: 50,
                };
                response_sender.send(Some((0, update_friend))).unwrap();
            },
            _ => unreachable!(),
        };

        // IndexClient will send to the server the information about the 0xaa friend:
        match await!(control_receiver.next()).unwrap() {
            SingleClientControl::SendMutations(_) => {},
            _ => unreachable!(),
        };

        (control_receiver, close_sender)
    }

    /// Add an index server to the IndexClient (From AppServer)
    async fn add_index_server(&mut self, server_address: ISA) {

        await!(self.app_server_sender.send(AppServerToIndexClient::AddIndexServer(server_address.clone()))).unwrap();

        let request = await!(self.database_receiver.next()).unwrap();
        assert_eq!(request.address, IndexClientConfigMutation::AddIndexServer(server_address.clone()));
        request.reply(Some(()));

        match await!(self.app_server_receiver.next()).unwrap() {
            IndexClientToAppServer::ReportMutations(mut mutations) => {
                assert_eq!(mutations.len(), 1);
                match mutations.pop().unwrap() {
                    IndexClientReportMutation::AddIndexServer(address) => 
                        assert_eq!(address, server_address),
                    _ => unreachable!(),
                };
            },
            _ => unreachable!(),
        };
    }

    /// Remove an index server to the IndexClient (From AppServer)
    async fn remove_index_server(&mut self, server_address: ISA)  {
        await!(self.app_server_sender.send(AppServerToIndexClient::RemoveIndexServer(server_address.clone()))).unwrap();

        let request = await!(self.database_receiver.next()).unwrap();
        assert_eq!(request.address, IndexClientConfigMutation::RemoveIndexServer(server_address.clone()));
        request.reply(Some(()));

        match await!(self.app_server_receiver.next()).unwrap() {
            IndexClientToAppServer::ReportMutations(mut mutations) => {
                assert_eq!(mutations.len(), 1);
                match mutations.pop().unwrap() {
                    IndexClientReportMutation::RemoveIndexServer(address) => 
                        assert_eq!(address, server_address.clone()),
                    _ => unreachable!(),
                };
            },
            _ => unreachable!(),
        };
    }
}


async fn task_index_client_loop_add_remove_index_server<S>(spawner: S) 
where   
    S: Spawn + Clone + Send + 'static,
{

    let mut icc = basic_index_client(spawner.clone());

    let (mut control_receiver, close_sender) = await!(icc.expect_server_connection(0x1337));

    await!(icc.add_index_server(0x1338));
    await!(icc.add_index_server(0x1339));
    await!(icc.remove_index_server(0x1338));
    // Remove index server in use (0x1337):
    await!(icc.remove_index_server(0x1337));

    
    // We expect that control_receiver will be closed eventually:
    while let Some(_control_message) = await!(control_receiver.next()) { 
    }

    // close_sender should notify that the connection was closed:
    let _ = close_sender.send(Ok(()));

    await!(icc.expect_set_connected_server(None));

    // IndexClient will wait backoff_ticks time before 
    // attempting to connect to the next index server:
    for _ in 0 .. icc.backoff_ticks {
        await!(icc.tick_sender.send(())).unwrap();
    }

    // Wait for a connection request:
    let session_conn_request = await!(icc.session_receiver.next()).unwrap();
    assert_eq!(session_conn_request.address, 0x1339);

    // Send a SessionHandle back to the index client:
    let (control_sender, _control_receiver) = mpsc::channel(0);
    let (_close_sender, close_receiver) = oneshot::channel();
    session_conn_request.reply(Some((control_sender, close_receiver)));

    // A new connection should be made to 0x1339:
    await!(icc.expect_set_connected_server(Some(0x1339)));
}


#[test]
fn test_index_client_loop_add_remove_index_server() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_add_remove_index_server(thread_pool.clone()));
}



async fn task_index_client_loop_apply_mutations<S>(spawner: S) 
where   
    S: Spawn + Clone + Send + 'static,
{

    let mut icc = basic_index_client(spawner.clone());
    let (mut control_receiver, _close_sender) = await!(icc.expect_server_connection(0x1337));

    let update_friend = UpdateFriend {
        public_key: PublicKey::from(PublicKey::from(&[0xbb; PUBLIC_KEY_LEN])),
        send_capacity: 200,
        recv_capacity: 100,

    };
    let index_mutation = IndexMutation::UpdateFriend(update_friend);
    let mutations = vec![index_mutation.clone()];
    await!(icc.app_server_sender.send(
            AppServerToIndexClient::ApplyMutations(mutations))).unwrap();

    // Wait for a request to mutate seq_friends:
    match await!(icc.seq_friends_receiver.next()).unwrap() {
        SeqFriendsRequest::Mutate(index_mutation0, response_sender) => {
            assert_eq!(index_mutation0, index_mutation);
            response_sender.send(()).unwrap();
        },
        _ => unreachable!(),
    };

    // Wait for a request for next update from seq_friends.
    // This is the one extra sequential friend update sent with our update:
    let next_update_friend = UpdateFriend {
        public_key: PublicKey::from(PublicKey::from(&[0xcc; PUBLIC_KEY_LEN])),
        send_capacity: 20,
        recv_capacity: 30,
    };

    match await!(icc.seq_friends_receiver.next()).unwrap() {
        SeqFriendsRequest::NextUpdate(response_sender) => {
            response_sender.send(Some((0, next_update_friend.clone()))).unwrap();
        },
        _ => unreachable!(),
    };

    // Wait for SendMutations:
    match await!(control_receiver.next()).unwrap() {
        SingleClientControl::SendMutations(mutations0) => {
            let next_update = IndexMutation::UpdateFriend(next_update_friend);
            assert_eq!(mutations0, vec![index_mutation, next_update]);
        },
        _ => unreachable!(),
    };
}

#[test]
fn test_index_client_loop_apply_mutations() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_apply_mutations(thread_pool.clone()));
}



async fn task_index_client_loop_request_routes_basic<S>(spawner: S) 
where   
    S: Spawn + Clone + Send + 'static,
{

    let mut icc = basic_index_client(spawner.clone());
    let (mut control_receiver, _close_sender) = await!(icc.expect_server_connection(0x1337));


    let request_routes = RequestRoutes {
        request_id: Uid::from(&[3; UID_LEN]),
        capacity: 250,
        source: PublicKey::from(PublicKey::from(&[0xee; PUBLIC_KEY_LEN])),
        destination: PublicKey::from(PublicKey::from(&[0xff; PUBLIC_KEY_LEN])),
        opt_exclude: None,
    };

    // Request routes from IndexClient (From AppServer):
    await!(icc.app_server_sender.send(
            AppServerToIndexClient::RequestRoutes(request_routes.clone()))).unwrap();

    // IndexClient forwards the routes request to the server:
    match await!(control_receiver.next()).unwrap() {
        SingleClientControl::RequestRoutes((request_routes0, response_sender)) => {
            assert_eq!(request_routes0, request_routes);
            // Server returns: no routes found:
            response_sender.send(vec![]).unwrap();
        },
        _ => unreachable!(),
    };

    // IndexClient returns the result back to AppServer:
    match await!(icc.app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ResponseRoutes(client_response_routes) => {
            assert_eq!(client_response_routes.request_id, Uid::from(&[3; UID_LEN]));
            let routes = match client_response_routes.result {
                ResponseRoutesResult::Success(routes) => routes,
                _ => unreachable!(),
            };
            assert!(routes.is_empty());
        },
        _ => unreachable!(),
    };
}

#[test]
fn test_index_client_loop_request_routes_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_request_routes_basic(thread_pool.clone()));
}


async fn task_index_client_loop_connecting_state<S>(spawner: S) 
where   
    S: Spawn + Clone + Send + 'static,
{
    let mut icc = basic_index_client(spawner.clone());

    // Wait for a connection request:
    let session_conn_request = await!(icc.session_receiver.next()).unwrap();
    assert_eq!(session_conn_request.address, 0x1337);

    // We got a connection request, but we don't reply.
    // This leaves IndexClient in "Connecting" state, where it is not yet connected to a server.
    //
    // During the "Connecting" state we expect that IndexClient 
    // will drop ApplyMuatations messages:
    
    let update_friend = UpdateFriend {
        public_key: PublicKey::from(PublicKey::from(&[0xbb; PUBLIC_KEY_LEN])),
        send_capacity: 200,
        recv_capacity: 100,

    };
    let index_mutation = IndexMutation::UpdateFriend(update_friend);
    let mutations = vec![index_mutation.clone()];
    await!(icc.app_server_sender.send(
            AppServerToIndexClient::ApplyMutations(mutations))).unwrap();
    
    // Wait for a request to mutate seq_friends. Note that this happens 
    // even in the "Connecting" state:
    match await!(icc.seq_friends_receiver.next()).unwrap() {
        SeqFriendsRequest::Mutate(index_mutation0, response_sender) => {
            assert_eq!(index_mutation0, index_mutation);
            response_sender.send(()).unwrap();
        },
        _ => unreachable!(),
    };
    
    // During the "Connecting" state we expect that IndexClient
    // will return a failure for RequestRoutes messages:
    
    let request_routes = RequestRoutes {
        request_id: Uid::from(&[3; UID_LEN]),
        capacity: 250,
        source: PublicKey::from(PublicKey::from(&[0xee; PUBLIC_KEY_LEN])),
        destination: PublicKey::from(PublicKey::from(&[0xff; PUBLIC_KEY_LEN])),
        opt_exclude: None,
    };

    // Request routes from IndexClient (From AppServer):
    await!(icc.app_server_sender.send(
            AppServerToIndexClient::RequestRoutes(request_routes.clone()))).unwrap();

    // IndexClient returns failure in ResponseRoutes:
    match await!(icc.app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ResponseRoutes(client_response_routes) => {
            assert_eq!(client_response_routes.request_id, Uid::from(&[3; UID_LEN]));
            match client_response_routes.result {
                ResponseRoutesResult::Failure => {},
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

}

#[test]
fn test_index_client_loop_connecting_state() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_connecting_state(thread_pool.clone()));
}
