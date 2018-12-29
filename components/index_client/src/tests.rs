use futures::task::{Spawn, SpawnExt};
use futures::executor::ThreadPool;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, TryFutureExt, StreamExt, SinkExt};

use common::dummy_connector::DummyConnector;

use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::hash::{HashResult, HASH_RESULT_LEN};
use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexClientReportMutation, UpdateFriend,
                                    IndexMutation};

use crate::index_client::{index_client_loop, IndexClientConfig,
                        IndexClientConfigMutation};
use crate::seq_friends::{SeqFriendsClient, SeqFriendsRequest};
use crate::single_client::SingleClientControl;


async fn task_index_client_loop_basic<S>(mut spawner: S) 
where   
    S: Spawn + Clone + Send + 'static,
{

    let (mut app_server_sender, from_app_server) = mpsc::channel(0);
    let (to_app_server, mut app_server_receiver) = mpsc::channel(0);

    let index_client_config = IndexClientConfig {
        index_servers: vec![0x1337u32],
    };

    let (seq_friends_sender, mut seq_friends_receiver) = mpsc::channel(0);
    let seq_friends_client = SeqFriendsClient::new(seq_friends_sender);

    let (session_sender, mut session_receiver) = mpsc::channel(0);
    let index_client_session = DummyConnector::new(session_sender);

    let (database_sender, mut database_receiver) = mpsc::channel(0);
    let database = DummyConnector::new(database_sender);

    let max_open_requests = 2;
    let keepalive_ticks = 8;
    let backoff_ticks = 4;

    let (mut tick_sender, timer_stream) = mpsc::channel::<()>(0);

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

    // Wait for a connection request:
    let session_conn_request = await!(session_receiver.next()).unwrap();
    assert_eq!(session_conn_request.address, 0x1337);

    // Send a SessionHandle back to the index client:
    let (control_sender, mut control_receiver) = mpsc::channel(0);
    let (close_sender, close_receiver) = oneshot::channel();
    session_conn_request.reply(Some((control_sender, close_receiver)));

    // We should be notified that a connection to a server was established:
    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::SetConnectedServer(Some(address)) => 
                    assert_eq!(address, 0x1337),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    match await!(seq_friends_receiver.next()).unwrap() {
        SeqFriendsRequest::ResetCountdown(response_sender) => {
            response_sender.send(()).unwrap();
        },
        _ => unreachable!(),
    };

    match await!(seq_friends_receiver.next()).unwrap() {
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

    // Add an index server (0x1338):
    await!(app_server_sender.send(AppServerToIndexClient::AddIndexServer(0x1338))).unwrap();

    let request = await!(database_receiver.next()).unwrap();
    assert_eq!(request.address, IndexClientConfigMutation::AddIndexServer(0x1338));
    request.reply(Some(()));

    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::AddIndexServer(address) => 
                    assert_eq!(address, 0x1338),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // Add an index server (0x1339):
    await!(app_server_sender.send(AppServerToIndexClient::AddIndexServer(0x1339))).unwrap();

    let request = await!(database_receiver.next()).unwrap();
    assert_eq!(request.address, IndexClientConfigMutation::AddIndexServer(0x1339));
    request.reply(Some(()));

    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::AddIndexServer(address) => 
                    assert_eq!(address, 0x1339),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // Remove index server (0x1338):
    await!(app_server_sender.send(AppServerToIndexClient::RemoveIndexServer(0x1338))).unwrap();

    let request = await!(database_receiver.next()).unwrap();
    assert_eq!(request.address, IndexClientConfigMutation::RemoveIndexServer(0x1338));
    request.reply(Some(()));

    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::RemoveIndexServer(address) => 
                    assert_eq!(address, 0x1338),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // Remove index server in use (0x1337):
    await!(app_server_sender.send(AppServerToIndexClient::RemoveIndexServer(0x1337))).unwrap();

    let request = await!(database_receiver.next()).unwrap();
    assert_eq!(request.address, IndexClientConfigMutation::RemoveIndexServer(0x1337));
    request.reply(Some(()));

    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::RemoveIndexServer(address) => 
                    assert_eq!(address, 0x1337),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // IndexClient will wait backoff_ticks time before 
    // attempting to connect to the next index server:

    match await!(control_receiver.next()).unwrap() {
        SingleClientControl::SendMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            let mutation = mutations.pop().unwrap();
            match mutation {
                IndexMutation::UpdateFriend(_) => {},
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // TODO: Some nondeterminstic issue here. To be fixed:
    
    assert!(await!(control_receiver.next()).is_none()); // Send Mutations
    close_sender.send(Ok(()));

    for _ in 0 .. backoff_ticks {
        await!(tick_sender.send(())).unwrap();
    }

    println!("Wait for a connection request:");

    // Wait for a connection request:
    let session_conn_request = await!(session_receiver.next()).unwrap();
    assert_eq!(session_conn_request.address, 0x1339);

    // Send a SessionHandle back to the index client:
    let (control_sender, control_receiver) = mpsc::channel(0);
    let (close_sender, close_receiver) = oneshot::channel();
    session_conn_request.reply(Some((control_sender, close_receiver)));

    println!("Wait for SetConnectedServer:");

    // A new connection should be made to 0x1339:
    match await!(app_server_receiver.next()).unwrap() {
        IndexClientToAppServer::ReportMutations(mut mutations) => {
            assert_eq!(mutations.len(), 1);
            match mutations.pop().unwrap() {
                IndexClientReportMutation::SetConnectedServer(Some(address)) => 
                    assert_eq!(address, 0x1339),
                _ => unreachable!(),
            };
        },
        _ => unreachable!(),
    };

    // TODO: Continue test here.
}


#[test]
fn test_index_client_loop_basic() {
    let _ = env_logger::init();
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_basic(thread_pool.clone()));
}
