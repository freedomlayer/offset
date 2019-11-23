use std::collections::HashMap;

use futures::{Stream, StreamExt};
use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use timer::create_timer_incoming;

use app::conn::{ConnPairApp, self};
use app::report::NodeReport;

use crate::utils::{
    advance_time, create_app, create_node, create_relay, named_relay_address, node_public_key,
    relay_address, relay_public_key, SimDb, report_service,
};

use crate::app_wrapper::send_request;
use crate::sim_network::create_sim_network;


const TIMER_CHANNEL_LEN: usize = 0;

/// Checks if a friend is online
/// panics if the friend does not exist.
async fn wait_friend_online(reports: &mut (impl Stream<Item=NodeReport> + Unpin), index: u8) {
    while let Some(node_report) = reports.next().await {
        let friend_report = match node_report
            .funder_report
            .friends
            .get(&node_public_key(index))
        {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness.is_online() {
            return;
        }
    }
    unreachable!();
}

/// Checks if a friend is online
/// panics if the friend does not exist.
async fn wait_friend_offline(reports: &mut (impl Stream<Item=NodeReport> + Unpin), index: u8) {
    while let Some(node_report) = reports.next().await {
        let friend_report = match node_report
            .funder_report
            .friends
            .get(&node_public_key(index))
        {
            None => unreachable!(),
            Some(friend_report) => friend_report,
        };
        if !friend_report.liveness.is_online() {
            return;
        }
    }
    unreachable!();
}

async fn task_relay_migration(mut test_executor: TestExecutor) {
    // Create timer_client:
    let (mut tick_sender, tick_receiver) = mpsc::channel(TIMER_CHANNEL_LEN);
    let timer_client = create_timer_incoming(tick_receiver, test_executor.clone()).unwrap();

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();

    // Create a database manager at the temporary directory:
    let sim_db = SimDb::new(temp_dir.path().to_path_buf());

    // A network simulator:
    let sim_net_client = create_sim_network(&mut test_executor);

    // Create initial database for node 0:
    sim_db.init_db(0);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(
        0,
        AppPermissions {
            routes: true,
            buyer: true,
            seller: true,
            config: true,
        },
    );

    let _node0_handle = create_node(
        0,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone(),
    )
    .await;

    let app0 = create_app(
        0,
        sim_net_client.clone(),
        timer_client.clone(),
        0,
        test_executor.clone(),
    )
    .await
    .unwrap();

    // Create initial database for node 1:
    sim_db.init_db(1);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(
        1,
        AppPermissions {
            routes: true,
            buyer: true,
            seller: true,
            config: true,
        },
    );
    let node1_handle = create_node(
        1,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone(),
    )
    .await;

    let app1 = create_app(
        1,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone(),
    )
    .await
    .unwrap();

    // Create relays:
    create_relay(
        0,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone(),
    )
    .await;

    create_relay(
        1,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone(),
    )
    .await;

    create_relay(
        2,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone(),
    )
    .await;

    create_relay(
        3,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone(),
    )
    .await;

    create_relay(
        4,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone(),
    )
    .await;


    let (_permissions0, node_report0, conn_pair0) = app0;
    let (_permissions1, node_report1, conn_pair1) = app1;

    let (sender0, receiver0) = conn_pair0.split();
    let (receiver0, mut reports0) = report_service(node_report0, receiver0, &test_executor);
    let mut conn_pair0 = ConnPairApp::from_raw(sender0, receiver0);

    let (sender1, receiver1) = conn_pair1.split();
    let (receiver1, mut reports1) = report_service(node_report1, receiver1, &test_executor);
    let mut conn_pair1 = ConnPairApp::from_raw(sender1, receiver1);

    // Configure relays:
    send_request(&mut conn_pair0, conn::config::add_relay(named_relay_address(0))).await.unwrap();
    send_request(&mut conn_pair1, conn::config::add_relay(named_relay_address(1))).await.unwrap();

    // Wait some time:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Add node1 as a friend:
    send_request(&mut conn_pair0, conn::config::add_friend(
            node_public_key(1),
            vec![relay_address(1)],
            String::from("node1"))).await.unwrap();

    // Node1: Add node0 as a friend:
    send_request(&mut conn_pair1, conn::config::add_friend(
            node_public_key(0),
            vec![relay_address(0)],
            String::from("node0"))).await.unwrap();


    send_request(&mut conn_pair0, conn::config::enable_friend(node_public_key(1))).await.unwrap();
    send_request(&mut conn_pair1, conn::config::enable_friend(node_public_key(0))).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    wait_friend_online(&mut reports0, 1).await;
    wait_friend_online(&mut reports1, 0).await;

    // Change relays for node0:
    send_request(&mut conn_pair0, conn::config::add_relay(named_relay_address(2))).await.unwrap();
    send_request(&mut conn_pair0, conn::config::add_relay(named_relay_address(0))).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Close node1:
    drop(node1_handle);

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0 should see Node1 as offline:
    wait_friend_offline(&mut reports0, 1).await;

    // App can not communicate with node1:
    assert!(send_request(&mut conn_pair1, conn::config::add_relay(named_relay_address(2))).await.is_err());

    // Change relays for node0 while node1 is offline:
    send_request(&mut conn_pair0, conn::config::add_relay(named_relay_address(4))).await.unwrap();
    send_request(&mut conn_pair0, conn::config::remove_relay(relay_public_key(2))).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Reopen node1:
    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(
        1,
        AppPermissions {
            routes: true,
            buyer: true,
            seller: true,
            config: true,
        },
    );
    let _node1_handle = create_node(
        1,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone(),
    )
    .await;

    // Connect an app to node1:
    let app1 = create_app(
        1,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone(),
    )
    .await
    .unwrap();

    let (_permissions1, node_report1, conn_pair1) = app1;
    let (_sender1, receiver1) = conn_pair1.split();
    let (_receiver1, mut reports1) = report_service(node_report1, receiver1, &test_executor);
    // let _conn_pair1 = ConnPairApp::from_raw(sender1, receiver1);

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node1 should be able to achieve connectivity:
    wait_friend_online(&mut reports0, 1).await;
    wait_friend_online(&mut reports1, 0).await;
}

#[test]
fn test_relay_migration() {
    // let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_relay_migration(test_executor.clone()));
    assert!(res.is_output());
}
