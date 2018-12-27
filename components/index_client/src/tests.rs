use futures::task::{Spawn, SpawnExt};
use futures::executor::ThreadPool;
use futures::channel::mpsc;
use futures::{FutureExt, TryFutureExt};

use common::dummy_connector::DummyConnector;

use crate::index_client::{index_client_loop, IndexClientConfig};
use crate::seq_friends::SeqFriendsClient;


async fn task_index_client_loop_basic<S>(mut spawner: S) 
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

    let (tick_sender, timer_stream) = mpsc::channel::<()>(0);

    let loop_fut = index_client_loop(from_app_server,
                               to_app_server,
                               index_client_config,
                               seq_friends_client,
                               index_client_session,
                               max_open_requests,
                               keepalive_ticks,
                               database,
                               timer_stream,
                               spawner.clone())
        .map_err(|e| error!("index_client_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut).unwrap();


    // TODO: Continue test here.
}


#[test]
fn test_index_client_loop_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_index_client_loop_basic(thread_pool.clone()));
}
