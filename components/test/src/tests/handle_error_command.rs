use std::collections::HashMap;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;

use timer::create_timer_incoming;

use app::conn::{self, ConnPairApp};

use crate::app_wrapper::send_request;
use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, create_app, create_node, create_relay, named_relay_address, node_public_key,
    relay_address, report_service, SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

async fn task_handle_error_command(mut test_executor: TestExecutor) {
    /*
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
    let currency3 = Currency::try_from("FST3".to_owned()).unwrap();
    */

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
    sim_db.init_node_db(0).unwrap();

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

    create_node(
        0,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone(),
    )
    .await
    .forget();

    // Connection attempt to the wrong node should fail:
    let opt_wrong_app = create_app(
        0,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone(),
    )
    .await;
    assert!(opt_wrong_app.is_none());

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
    sim_db.init_node_db(1).unwrap();

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
    create_node(
        1,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone(),
    )
    .await
    .forget();

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

    let (_permissions0, node_report0, conn_pair0) = app0;
    let (_permissions1, node_report1, conn_pair1) = app1;

    let (sender0, receiver0) = conn_pair0.split();
    let (receiver0, mut _report_client0) = report_service(node_report0, receiver0, &test_executor);
    let mut conn_pair0 = ConnPairApp::from_raw(sender0, receiver0);

    let (sender1, receiver1) = conn_pair1.split();
    let (receiver1, mut _report_client1) = report_service(node_report1, receiver1, &test_executor);
    let mut conn_pair1 = ConnPairApp::from_raw(sender1, receiver1);

    // Configure relays:
    send_request(
        &mut conn_pair0,
        conn::config::add_relay(named_relay_address(0)),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::add_relay(named_relay_address(1)),
    )
    .await
    .unwrap();

    // Wait some time:
    advance_time(20, &mut tick_sender, &test_executor).await;

    // Node0: Add node1 as a friend:
    send_request(
        &mut conn_pair0,
        conn::config::add_friend(
            node_public_key(1),
            vec![relay_address(1)],
            String::from("node1"),
        ),
    )
    .await
    .unwrap();

    // Node1: Add node0 as a friend:
    send_request(
        &mut conn_pair1,
        conn::config::add_friend(
            node_public_key(0),
            vec![relay_address(0)],
            String::from("node0"),
        ),
    )
    .await
    .unwrap();

    // Node0: Enable node1:
    send_request(
        &mut conn_pair0,
        conn::config::enable_friend(node_public_key(1)),
    )
    .await
    .unwrap();

    // Node0: Enable node0. This is an intentional mistake, it should not work.
    // We want to make sure that such mistakes are handled properly.
    send_request(
        &mut conn_pair0,
        conn::config::enable_friend(node_public_key(0)),
    )
    .await
    .unwrap();

    advance_time(10, &mut tick_sender, &test_executor).await;
}

#[test]
fn test_handle_error_command() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_handle_error_command(test_executor.clone()));
    assert!(res.is_output());
}
