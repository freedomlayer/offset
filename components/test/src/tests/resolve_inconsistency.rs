use std::collections::HashMap;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::report::messages::ChannelStatusReport;
use timer::create_timer_incoming;

use crate::utils::{
    advance_time, create_app, create_node, create_relay, named_relay_address, node_public_key,
    relay_address, SimDb,
};

use crate::sim_network::create_sim_network;

const TIMER_CHANNEL_LEN: usize = 0;

async fn task_resolve_inconsistency(mut test_executor: TestExecutor) {
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
            100,
        )
        .await
        .unwrap();

    // Node1: Add node0 as a friend:
    config1
        .add_friend(
            node_public_key(0),
            vec![relay_address(0)],
            String::from("node0"),
            -100,
        )
        .await
        .unwrap();

    config0.enable_friend(node_public_key(1)).await.unwrap();
    config1.enable_friend(node_public_key(0)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    config0.open_friend(node_public_key(1)).await.unwrap();
    config1.open_friend(node_public_key(0)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: remove the friend node1:
    config0.remove_friend(node_public_key(1)).await.unwrap();

    // Node0: Add node1 as a friend with a different balance
    // This should cause an inconsistency when the first token
    // message will be sent.
    config0
        .add_friend(
            node_public_key(1),
            vec![relay_address(1)],
            String::from("node1"),
            50,
        )
        .await
        .unwrap();

    // Node0 enables the friend node1. This is required to trigger the inconsistency error
    config0.enable_friend(node_public_key(1)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node1 should now perceive the mutual channel with node0 to be inconsistent:
    {
        let (node_report, _) = report1.incoming_reports().await.unwrap();
        let friend_report = node_report
            .funder_report
            .friends
            .get(&node_public_key(0))
            .unwrap();

        let incon_report = match &friend_report.channel_status {
            ChannelStatusReport::Consistent(_) => unreachable!(),
            ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
                channel_inconsistent_report
            }
        };

        assert_eq!(incon_report.local_reset_terms_balance, -100);
        let remote_reset_terms = incon_report.opt_remote_reset_terms.clone().unwrap();
        assert_eq!(remote_reset_terms.balance_for_reset, 50);
    }

    let (node_report, _) = report0.incoming_reports().await.unwrap();
    let friend_report = node_report
        .funder_report
        .friends
        .get(&node_public_key(1))
        .unwrap();

    let incon_report = match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => unreachable!(),
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            channel_inconsistent_report
        }
    };

    assert_eq!(incon_report.local_reset_terms_balance, 50);
    let remote_reset_terms = incon_report.opt_remote_reset_terms.clone().unwrap();
    assert_eq!(remote_reset_terms.balance_for_reset, -100);

    let reset_token = remote_reset_terms.reset_token.clone();

    // Node0 agrees to the conditions of node1:
    config0
        .reset_friend_channel(node_public_key(1), reset_token)
        .await
        .unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Channel should be consistent now:
    let (node_report, _) = report0.incoming_reports().await.unwrap();
    let friend_report = node_report
        .funder_report
        .friends
        .get(&node_public_key(1))
        .unwrap();

    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => {}
        ChannelStatusReport::Inconsistent(_) => unreachable!(),
    };

    // Node1: Channel should be consistent now:
    let (node_report, _) = report1.incoming_reports().await.unwrap();
    let friend_report = node_report
        .funder_report
        .friends
        .get(&node_public_key(0))
        .unwrap();

    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => {}
        ChannelStatusReport::Inconsistent(_) => unreachable!(),
    };

    // Let both sides open the channel:
    config0.open_friend(node_public_key(1)).await.unwrap();
    config1.open_friend(node_public_key(0)).await.unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Make sure again that the channel stays consistent:
    // Node0: Channel should be consistent now:
    let (node_report, _) = report0.incoming_reports().await.unwrap();
    let friend_report = node_report
        .funder_report
        .friends
        .get(&node_public_key(1))
        .unwrap();

    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => {}
        ChannelStatusReport::Inconsistent(_) => unreachable!(),
    };

    // Node1: Channel should be consistent now:
    let (node_report, _) = report1.incoming_reports().await.unwrap();
    let friend_report = node_report
        .funder_report
        .friends
        .get(&node_public_key(0))
        .unwrap();

    match &friend_report.channel_status {
        ChannelStatusReport::Consistent(_) => {}
        ChannelStatusReport::Inconsistent(_) => unreachable!(),
    };
}

#[test]
fn test_resolve_inconsistency() {
    // let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_resolve_inconsistency(test_executor.clone()));
    assert!(res.is_output());
}
