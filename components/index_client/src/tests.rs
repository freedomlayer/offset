use std::convert::TryFrom;

use futures::channel::{mpsc, oneshot};
use futures::executor::{block_on, ThreadPool};
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};

use common::dummy_connector::{ConnRequest, DummyConnector};

use proto::crypto::{PublicKey, Uid};

use proto::funder::messages::{Currency, Rate};
use proto::index_client::messages::{
    AppServerToIndexClient, IndexClientReportMutation, IndexClientRequest, IndexClientToAppServer,
    IndexMutation, RequestRoutes, ResponseRoutesResult, UpdateFriendCurrency,
};
use proto::index_server::messages::{IndexServerAddress, NamedIndexServerAddress};

use database::{DatabaseClient, DatabaseRequest};

use crate::client_session::SessionHandle;
use crate::index_client::{index_client_loop, IndexClientConfig, IndexClientConfigMutation};
use crate::seq_friends::{SeqFriendsClient, SeqFriendsRequest};
use crate::single_client::{SingleClientControl, SingleClientError};

/// A test util struct
/// Holds sender/receiver interface for an IndexClient.
struct IndexClientControl<ISA> {
    app_server_sender: mpsc::Sender<AppServerToIndexClient<ISA>>,
    app_server_receiver: mpsc::Receiver<IndexClientToAppServer<ISA>>,
    seq_friends_receiver: mpsc::Receiver<SeqFriendsRequest>,
    session_receiver: mpsc::Receiver<ConnRequest<IndexServerAddress<ISA>, Option<SessionHandle>>>,
    database_req_receiver: mpsc::Receiver<DatabaseRequest<IndexClientConfigMutation<ISA>>>,
    tick_sender: mpsc::Sender<()>,
    // TODO: Check: Why is this field unused?
    #[allow(unused)]
    max_open_requests: usize,
    // TODO: Check: Why is this field unused?
    #[allow(unused)]
    keepalive_ticks: usize,
    backoff_ticks: usize,
}

/// Create a basic IndexClientControl, used for testing
fn basic_index_client<S>(spawner: S) -> IndexClientControl<u32>
where
    S: Spawn + Clone + Send + 'static,
{
    let (app_server_sender, from_app_server) = mpsc::channel(1);
    let (to_app_server, app_server_receiver) = mpsc::channel(1);

    let index_server37 = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0x37; PublicKey::len()]),
        address: 0x1337u32,
        name: "0x1337".to_owned(),
    };
    let index_client_config = IndexClientConfig {
        index_servers: vec![index_server37],
    };

    let (seq_friends_sender, seq_friends_receiver) = mpsc::channel(0);
    let seq_friends_client = SeqFriendsClient::new(seq_friends_sender);

    let (session_sender, session_receiver) = mpsc::channel(0);
    let index_client_session = DummyConnector::new(session_sender);

    let (database_req_sender, database_req_receiver) = mpsc::channel(0);
    let db_client = DatabaseClient::new(database_req_sender);

    let max_open_requests = 2;
    let keepalive_ticks = 8;
    let backoff_ticks = 4;

    let (tick_sender, timer_stream) = mpsc::channel::<()>(0);

    let loop_fut = index_client_loop(
        from_app_server,
        to_app_server,
        index_client_config,
        seq_friends_client,
        index_client_session,
        max_open_requests,
        keepalive_ticks,
        backoff_ticks,
        db_client,
        timer_stream,
        spawner.clone(),
    )
    .map_err(|e| error!("index_client_loop() error: {:?}", e))
    .map(|_| ());

    spawner.spawn(loop_fut).unwrap();

    IndexClientControl {
        app_server_sender,
        app_server_receiver,
        seq_friends_receiver,
        session_receiver,
        database_req_receiver,
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
    async fn expect_set_connected_server(&mut self, expected_opt_public_key: Option<PublicKey>) {
        match self.app_server_receiver.next().await.unwrap() {
            IndexClientToAppServer::ReportMutations(mut ic_report_mutations) => {
                assert_eq!(ic_report_mutations.opt_app_request_id, None);
                assert_eq!(ic_report_mutations.mutations.len(), 1);
                match ic_report_mutations.mutations.pop().unwrap() {
                    IndexClientReportMutation::SetConnectedServer(opt_public_key) => {
                        assert_eq!(opt_public_key, expected_opt_public_key)
                    }
                    _ => unreachable!(),
                };
            }
            _ => unreachable!(),
        };
    }

    /// Expect a connection to index server of a certain public key
    async fn expect_server_connection(
        &mut self,
        index_server: IndexServerAddress<ISA>,
    ) -> (
        mpsc::Receiver<SingleClientControl>,
        oneshot::Sender<Result<(), SingleClientError>>,
    ) {
        // Wait for a connection request:
        let session_conn_request = self.session_receiver.next().await.unwrap();
        assert_eq!(session_conn_request.address, index_server);

        // Send a SessionHandle back to the index client:
        let (control_sender, mut control_receiver) = mpsc::channel(0);
        let (close_sender, close_receiver) = oneshot::channel();
        session_conn_request.reply(Some((control_sender, close_receiver)));

        // We should be notified that a connection to a server was established:
        self.expect_set_connected_server(Some(index_server.public_key))
            .await;

        match self.seq_friends_receiver.next().await.unwrap() {
            SeqFriendsRequest::ResetCountdown(response_sender) => {
                response_sender.send(()).unwrap();
            }
            _ => unreachable!(),
        };

        let currency = Currency::try_from("FST".to_owned()).unwrap();

        // Feeding the send_full_state() task with one friend state:
        match self.seq_friends_receiver.next().await.unwrap() {
            SeqFriendsRequest::NextUpdate(response_sender) => {
                let update_friend = UpdateFriendCurrency {
                    public_key: PublicKey::from(PublicKey::from(&[0xaa; PublicKey::len()])),
                    currency: currency.clone(),
                    recv_capacity: 50,
                    rate: Rate { mul: 0, add: 1 },
                };
                response_sender.send(Some((0, update_friend))).unwrap();
            }
            _ => unreachable!(),
        };

        // IndexClient will send to the server the information about the 0xaa friend:
        match control_receiver.next().await.unwrap() {
            SingleClientControl::SendMutations(_) => {}
            _ => unreachable!(),
        };

        (control_receiver, close_sender)
    }

    /// Add an index server to the IndexClient (From AppServer)
    async fn add_index_server(&mut self, named_index_server_address: NamedIndexServerAddress<ISA>) {
        let app_server_to_index_client = AppServerToIndexClient::AppRequest((
            Uid::from(&[52; Uid::len()]),
            IndexClientRequest::AddIndexServer(named_index_server_address.clone()),
        ));
        self.app_server_sender
            .send(app_server_to_index_client)
            .await
            .unwrap();

        let db_request = self.database_req_receiver.next().await.unwrap();
        assert_eq!(
            db_request.mutations,
            vec![IndexClientConfigMutation::AddIndexServer(
                named_index_server_address.clone()
            )]
        );
        db_request.response_sender.send(()).unwrap();

        match self.app_server_receiver.next().await.unwrap() {
            IndexClientToAppServer::ReportMutations(mut ic_report_mutations) => {
                assert_eq!(
                    ic_report_mutations.opt_app_request_id,
                    Some(Uid::from(&[52; Uid::len()]))
                );
                assert_eq!(ic_report_mutations.mutations.len(), 1);
                match ic_report_mutations.mutations.pop().unwrap() {
                    IndexClientReportMutation::AddIndexServer(named_index_server_address0) => {
                        assert_eq!(named_index_server_address0, named_index_server_address)
                    }
                    _ => unreachable!(),
                };
            }
            _ => unreachable!(),
        };
    }

    /// Remove an index server to the IndexClient (From AppServer)
    async fn remove_index_server(&mut self, public_key: PublicKey) {
        let app_server_to_index_client = AppServerToIndexClient::AppRequest((
            Uid::from(&[53; Uid::len()]),
            IndexClientRequest::RemoveIndexServer(public_key.clone()),
        ));
        self.app_server_sender
            .send(app_server_to_index_client)
            .await
            .unwrap();

        let db_request = self.database_req_receiver.next().await.unwrap();
        assert_eq!(
            db_request.mutations,
            vec![IndexClientConfigMutation::RemoveIndexServer(
                public_key.clone()
            )]
        );
        db_request.response_sender.send(()).unwrap();

        match self.app_server_receiver.next().await.unwrap() {
            IndexClientToAppServer::ReportMutations(mut ic_report_mutations) => {
                assert_eq!(
                    ic_report_mutations.opt_app_request_id,
                    Some(Uid::from(&[53; Uid::len()]))
                );
                assert_eq!(ic_report_mutations.mutations.len(), 1);
                match ic_report_mutations.mutations.pop().unwrap() {
                    IndexClientReportMutation::RemoveIndexServer(public_key0) => {
                        assert_eq!(public_key0, public_key.clone())
                    }
                    _ => unreachable!(),
                };
            }
            _ => unreachable!(),
        };
    }
}

async fn task_index_client_loop_add_remove_index_server<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let mut icc = basic_index_client(spawner.clone());

    let index_server = IndexServerAddress {
        public_key: PublicKey::from(&[0x37; PublicKey::len()]),
        address: 0x1337,
    };
    let (mut control_receiver, close_sender) = icc.expect_server_connection(index_server).await;

    icc.add_index_server(NamedIndexServerAddress {
        public_key: PublicKey::from(&[0x38; PublicKey::len()]),
        address: 0x1338,
        name: "0x1338".to_owned(),
    })
    .await;
    icc.add_index_server(NamedIndexServerAddress {
        public_key: PublicKey::from(&[0x39; PublicKey::len()]),
        address: 0x1339,
        name: "0x1339".to_owned(),
    })
    .await;
    icc.remove_index_server(PublicKey::from(&[0x38; PublicKey::len()]))
        .await;
    // Remove index server in use (0x1337):
    icc.remove_index_server(PublicKey::from(&[0x37; PublicKey::len()]))
        .await;

    // We expect that control_receiver will be closed eventually:
    while let Some(_control_message) = control_receiver.next().await {}

    // close_sender should notify that the connection was closed:
    let _ = close_sender.send(Ok(()));

    icc.expect_set_connected_server(None).await;

    // IndexClient will wait backoff_ticks time before
    // attempting to connect to the next index server:
    for _ in 0..icc.backoff_ticks {
        icc.tick_sender.send(()).await.unwrap();
    }

    // Wait for a connection request:
    let session_conn_request = icc.session_receiver.next().await.unwrap();
    let index_server = IndexServerAddress {
        public_key: PublicKey::from(&[0x39; PublicKey::len()]),
        address: 0x1339,
    };
    assert_eq!(session_conn_request.address, index_server);

    // Send a SessionHandle back to the index client:
    let (control_sender, _control_receiver) = mpsc::channel(0);
    let (_close_sender, close_receiver) = oneshot::channel();
    session_conn_request.reply(Some((control_sender, close_receiver)));

    // A new connection should be made to 0x1339:
    icc.expect_set_connected_server(Some(PublicKey::from(&[0x39; PublicKey::len()])))
        .await;
}

#[test]
fn test_index_client_loop_add_remove_index_server() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_index_client_loop_add_remove_index_server(
        thread_pool.clone(),
    ));
}

async fn task_index_client_loop_apply_mutations<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let mut icc = basic_index_client(spawner.clone());
    let index_server = IndexServerAddress {
        public_key: PublicKey::from(&[0x37; PublicKey::len()]),
        address: 0x1337,
    };
    let (mut control_receiver, _close_sender) = icc.expect_server_connection(index_server).await;

    let update_friend_currency = UpdateFriendCurrency {
        public_key: PublicKey::from(PublicKey::from(&[0xbb; PublicKey::len()])),
        currency: currency.clone(),
        recv_capacity: 100,
        rate: Rate { mul: 0, add: 1 },
    };
    let index_mutation = IndexMutation::UpdateFriendCurrency(update_friend_currency);
    let mutations = vec![index_mutation.clone()];
    icc.app_server_sender
        .send(AppServerToIndexClient::ApplyMutations(mutations))
        .await
        .unwrap();

    // Wait for a request to mutate seq_friends:
    match icc.seq_friends_receiver.next().await.unwrap() {
        SeqFriendsRequest::Mutate(index_mutation0, response_sender) => {
            assert_eq!(index_mutation0, index_mutation);
            response_sender.send(()).unwrap();
        }
        _ => unreachable!(),
    };

    // Wait for a request for next update from seq_friends.
    // This is the one extra sequential friend update sent with our update:
    let next_update_friend = UpdateFriendCurrency {
        public_key: PublicKey::from(PublicKey::from(&[0xcc; PublicKey::len()])),
        currency: currency.clone(),
        recv_capacity: 30,
        rate: Rate { mul: 0, add: 1 },
    };

    match icc.seq_friends_receiver.next().await.unwrap() {
        SeqFriendsRequest::NextUpdate(response_sender) => {
            response_sender
                .send(Some((0, next_update_friend.clone())))
                .unwrap();
        }
        _ => unreachable!(),
    };

    // Wait for SendMutations:
    match control_receiver.next().await.unwrap() {
        SingleClientControl::SendMutations(mutations0) => {
            let next_update = IndexMutation::UpdateFriendCurrency(next_update_friend);
            assert_eq!(mutations0, vec![index_mutation, next_update]);
        }
        _ => unreachable!(),
    };
}

#[test]
fn test_index_client_loop_apply_mutations() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_index_client_loop_apply_mutations(thread_pool.clone()));
}

async fn task_index_client_loop_request_routes_basic<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let currency = Currency::try_from("FST".to_owned()).unwrap();
    let mut icc = basic_index_client(spawner.clone());
    let index_server = IndexServerAddress {
        public_key: PublicKey::from(&[0x37; PublicKey::len()]),
        address: 0x1337,
    };
    let (mut control_receiver, _close_sender) = icc.expect_server_connection(index_server).await;

    let request_routes = RequestRoutes {
        request_id: Uid::from(&[3; Uid::len()]),
        currency: currency.clone(),
        capacity: 250,
        source: PublicKey::from(PublicKey::from(&[0xee; PublicKey::len()])),
        destination: PublicKey::from(PublicKey::from(&[0xff; PublicKey::len()])),
        opt_exclude: None,
    };

    // Request routes from IndexClient (From AppServer):
    let app_server_to_index_client = AppServerToIndexClient::AppRequest((
        Uid::from(&[50; Uid::len()]),
        IndexClientRequest::RequestRoutes(request_routes.clone()),
    ));
    icc.app_server_sender
        .send(app_server_to_index_client)
        .await
        .unwrap();

    // IndexClient forwards the routes request to the server:
    match control_receiver.next().await.unwrap() {
        SingleClientControl::RequestRoutes((request_routes0, response_sender)) => {
            assert_eq!(request_routes0, request_routes);
            // Server returns: no routes found:
            response_sender.send(vec![]).unwrap();
        }
        _ => unreachable!(),
    };

    // Expect empty report mutations:
    match icc.app_server_receiver.next().await.unwrap() {
        IndexClientToAppServer::ReportMutations(ic_report_mutations) => {
            assert_eq!(
                ic_report_mutations.opt_app_request_id,
                Some(Uid::from(&[50; Uid::len()]))
            );
            assert!(ic_report_mutations.mutations.is_empty());
        }
        _ => unreachable!(),
    };

    // IndexClient returns the result back to AppServer:
    match icc.app_server_receiver.next().await.unwrap() {
        IndexClientToAppServer::ResponseRoutes(client_response_routes) => {
            assert_eq!(
                client_response_routes.request_id,
                Uid::from(&[3; Uid::len()])
            );
            let routes = match client_response_routes.result {
                ResponseRoutesResult::Success(routes) => routes,
                _ => unreachable!(),
            };
            assert!(routes.is_empty());
        }
        _ => unreachable!(),
    };
}

#[test]
fn test_index_client_loop_request_routes_basic() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_index_client_loop_request_routes_basic(
        thread_pool.clone(),
    ));
}

async fn task_index_client_loop_connecting_state<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let currency = Currency::try_from("FST".to_owned()).unwrap();
    let mut icc = basic_index_client(spawner.clone());

    // Wait for a connection request:
    let session_conn_request = icc.session_receiver.next().await.unwrap();
    let index_server = IndexServerAddress {
        public_key: PublicKey::from(&[0x37; PublicKey::len()]),
        address: 0x1337,
    };
    assert_eq!(session_conn_request.address, index_server);

    // We got a connection request, but we don't reply.
    // This leaves IndexClient in "Connecting" state, where it is not yet connected to a server.
    //
    // During the "Connecting" state we expect that IndexClient
    // will drop ApplyMutations messages:

    let update_friend_currency = UpdateFriendCurrency {
        public_key: PublicKey::from(PublicKey::from(&[0xbb; PublicKey::len()])),
        currency: currency.clone(),
        recv_capacity: 100,
        rate: Rate { mul: 0, add: 1 },
    };
    let index_mutation = IndexMutation::UpdateFriendCurrency(update_friend_currency);
    let mutations = vec![index_mutation.clone()];
    icc.app_server_sender
        .send(AppServerToIndexClient::ApplyMutations(mutations))
        .await
        .unwrap();

    // Wait for a request to mutate seq_friends. Note that this happens
    // even in the "Connecting" state:
    match icc.seq_friends_receiver.next().await.unwrap() {
        SeqFriendsRequest::Mutate(index_mutation0, response_sender) => {
            assert_eq!(index_mutation0, index_mutation);
            response_sender.send(()).unwrap();
        }
        _ => unreachable!(),
    };

    // During the "Connecting" state we expect that IndexClient
    // will return a failure for RequestRoutes messages:

    let request_routes = RequestRoutes {
        request_id: Uid::from(&[3; Uid::len()]),
        currency: currency.clone(),
        capacity: 250,
        source: PublicKey::from(PublicKey::from(&[0xee; PublicKey::len()])),
        destination: PublicKey::from(PublicKey::from(&[0xff; PublicKey::len()])),
        opt_exclude: None,
    };

    // Request routes from IndexClient (From AppServer):
    let app_server_to_index_client = AppServerToIndexClient::AppRequest((
        Uid::from(&[51; Uid::len()]),
        IndexClientRequest::RequestRoutes(request_routes.clone()),
    ));
    icc.app_server_sender
        .send(app_server_to_index_client)
        .await
        .unwrap();

    // Expect empty report mutations:
    match icc.app_server_receiver.next().await.unwrap() {
        IndexClientToAppServer::ReportMutations(ic_report_mutations) => {
            assert_eq!(
                ic_report_mutations.opt_app_request_id,
                Some(Uid::from(&[51; Uid::len()]))
            );
            assert!(ic_report_mutations.mutations.is_empty());
        }
        _ => unreachable!(),
    };

    // IndexClient returns failure in ResponseRoutes:
    match icc.app_server_receiver.next().await.unwrap() {
        IndexClientToAppServer::ResponseRoutes(client_response_routes) => {
            assert_eq!(
                client_response_routes.request_id,
                Uid::from(&[3; Uid::len()])
            );
            match client_response_routes.result {
                ResponseRoutesResult::Failure => {}
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    };

    // Graceful shutdown:
    let IndexClientControl {
        app_server_sender,
        mut app_server_receiver,
        ..
    } = icc;
    drop(app_server_sender);
    assert!(app_server_receiver.next().await.is_none());
}

#[test]
fn test_index_client_loop_connecting_state() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_index_client_loop_connecting_state(thread_pool.clone()));
}

// TODO: Add more tests.
