use std::collections::HashMap;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use timer::create_timer_incoming;

use crate::utils::{
    advance_time, create_app, create_node, create_relay, named_relay_address, node_public_key,
    relay_address, relay_public_key, SimDb,
};

use app::conn::AppReport;

use crate::sim_network::create_sim_network;

const TIMER_CHANNEL_LEN: usize = 0;

/// Checks if a friend is online
/// panics if the friend does not exist.
async fn is_friend_online(report: &mut AppReport, index: u8) -> bool {
    let (node_report, mutations_receiver) = report.incoming_reports().await.unwrap();
    drop(mutations_receiver);

    let friend_report = match node_report
        .funder_report
        .friends
        .get(&node_public_key(index))
    {
        None => unreachable!(),
        Some(friend_report) => friend_report,
    };
    friend_report.liveness.is_online()
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

    let mut app0 = create_app(
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

    let mut app1 = create_app(
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

    let mut config0 = app0.config().unwrap().clone();
    let mut config1 = app1.config().unwrap().clone();

    let mut report0 = app0.report().clone();
    let mut report1 = app1.report().clone();

    // Configure relays:
    config0.add_relay(named_relay_address(0)).await.unwrap();
    config1.add_relay(named_relay_address(1)).await.unwrap();

    // Wait some time:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Add node1 as a friend:
    config0
        .add_friend(
            node_public_key(1),
            vec![relay_address(1)],
            String::from("node1"),
        )
        .await
        .unwrap();

    // Node1: Add node0 as a friend:
    config1
        .add_friend(
            node_public_key(0),
            vec![relay_address(0)],
            String::from("node0"),
        )
        .await
        .unwrap();

    config0.enable_friend(node_public_key(1)).await.unwrap();
    config1.enable_friend(node_public_key(0)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    assert!(is_friend_online(&mut report0, 1).await);
    assert!(is_friend_online(&mut report1, 0).await);

    // Change relays for node0:
    config0.add_relay(named_relay_address(2)).await.unwrap();
    config0.remove_relay(relay_public_key(0)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Close node1:
    drop(node1_handle);

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0 should see Node1 as offline:
    assert!(!is_friend_online(&mut report0, 1).await);
    // App can not communicate with node1:
    assert!(config1.add_relay(named_relay_address(2)).await.is_err());
    drop(app1);
    drop(report1);
    drop(config1);

    // Change relays for node0 while node1 is offline:
    config0.add_relay(named_relay_address(4)).await.unwrap();
    config0.remove_relay(relay_public_key(2)).await.unwrap();

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
    let mut app1 = create_app(
        1,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone(),
    )
    .await
    .unwrap();

    let mut report1 = app1.report().clone();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node1 should be able to achieve connectivity:
    assert!(is_friend_online(&mut report0, 1).await);
    assert!(is_friend_online(&mut report1, 0).await);
}

#[test]
fn test_relay_migration() {
    // let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_relay_migration(test_executor.clone()));
    assert!(res.is_output());
}
