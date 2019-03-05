use std::collections::HashMap;

use futures::channel::mpsc;
use futures::task::Spawn;
use futures::executor::ThreadPool;
use futures::{StreamExt, SinkExt};

use tempfile::tempdir;

use common::spawner_wait::SpawnerWait;

use timer::{create_timer_incoming};
use proto::app_server::messages::AppPermissions;
use proto::funder::messages::{InvoiceId, INVOICE_ID_LEN};

use crypto::uid::{Uid, UID_LEN};

use crate::utils::{create_node, create_app, SimDb,
                    create_relay, create_index_server,
                    relay_address, named_relay_address, 
                    named_index_server_address, node_public_key};
use crate::sim_network::create_sim_network;

const TIMER_CHANNEL_LEN: usize = 0;
// const YIELD_ITERS: usize = 0x1000;

async fn task_two_nodes_payment<S>(spawner: S) 
where
    S: Spawn + Clone + Send + Sync + 'static,
{

    let mut wspawner = SpawnerWait::new(spawner);

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();

    // Create a database manager at the temporary directory:
    let sim_db = SimDb::new(temp_dir.path().to_path_buf());

    // A network simulator:
    let sim_net_client = create_sim_network(&mut wspawner);

    // Create timer_client:
    let (mut tick_sender, tick_receiver) = mpsc::channel(TIMER_CHANNEL_LEN);
    let timer_client = create_timer_incoming(tick_receiver, wspawner.clone()).unwrap();


    // Create initial database for node 0:
    sim_db.init_db(0);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(0, AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    });

    await!(create_node(0, 
              sim_db.clone(),
              timer_client.clone(),
              sim_net_client.clone(),
              trusted_apps,
              wspawner.clone()));

    let mut app0 = await!(create_app(0,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    0,
                    wspawner.clone()));


    // Create initial database for node 1:
    sim_db.init_db(1);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(1, AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    });
    await!(create_node(1, 
              sim_db.clone(),
              timer_client.clone(),
              sim_net_client.clone(),
              trusted_apps,
              wspawner.clone()));

    let mut app1 = await!(create_app(1,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    1,
                    wspawner.clone()));

    // Create relays:
    await!(create_relay(0,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 wspawner.clone()));

    await!(create_relay(1,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 wspawner.clone()));
    
    // Create three index servers:
    // 0 -- 2 -- 1
    // The only way for information to flow between the two index servers
    // is by having the middle server forward it.
    await!(create_index_server(2,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![0,1],
                             wspawner.clone()));


    await!(create_index_server(0,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             wspawner.clone()));

    await!(create_index_server(1,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             wspawner.clone()));


    let mut config0 = app0.config().unwrap().clone();
    let mut config1 = app1.config().unwrap().clone();

    let mut routes0 = app0.routes().unwrap().clone();
    let mut routes1 = app1.routes().unwrap().clone();

    let mut send_funds0 = app0.send_funds().unwrap().clone();
    let mut send_funds1 = app1.send_funds().unwrap().clone();

    let mut report0 = app0.report().clone();
    let mut report1 = app1.report().clone();

    // Configure relays:
    await!(config0.add_relay(named_relay_address(0))).unwrap();
    await!(config1.add_relay(named_relay_address(1))).unwrap();

    // Configure index servers:
    await!(config0.add_index_server(named_index_server_address(0))).unwrap();
    await!(config1.add_index_server(named_index_server_address(1))).unwrap();

    // Wait some time:
    for _ in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(wspawner.wait()).unwrap();
    }

    // Node0: Add node1 as a friend:
    await!(config0.add_friend(node_public_key(1),
                              vec![relay_address(1)],
                              String::from("node1"),
                              100)).unwrap();

    // Node1: Add node0 as a friend:
    await!(config1.add_friend(node_public_key(0),
                              vec![relay_address(0)],
                              String::from("node0"),
                              -100)).unwrap();

    await!(config0.enable_friend(node_public_key(1))).unwrap();
    await!(config1.enable_friend(node_public_key(0))).unwrap();

    // Wait some time:
    for _ in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(wspawner.wait()).unwrap();
    }

    // Node0: Wait until node1 is online:
    let (mut node_report, mut mutations_receiver) = await!(report0.incoming_reports()).unwrap();
    loop {
        let friend_report = match node_report.funder_report.friends.get(&node_public_key(1)) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness.is_online() {
            break;
        }

        // Apply mutations:
        let mutations = await!(mutations_receiver.next()).unwrap();
        for mutation in mutations {
            node_report.mutate(&mutation).unwrap();
        }
    }
    drop(mutations_receiver);

    // Node1: Wait until node0 is online:
    let (mut node_report, mut mutations_receiver) = await!(report1.incoming_reports()).unwrap();
    loop {
        let friend_report = match node_report.funder_report.friends.get(&node_public_key(0)) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness.is_online() {
            break;
        }

        // Apply mutations:
        let mutations = await!(mutations_receiver.next()).unwrap();
        for mutation in mutations {
            node_report.mutate(&mutation).unwrap();
        }
    }
    drop(mutations_receiver);

    await!(config0.open_friend(node_public_key(1))).unwrap();
    await!(config1.open_friend(node_public_key(0))).unwrap();

    // Wait some time, to let the index servers exchange information:
    for _ in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(wspawner.wait()).unwrap();
    }

    // Node0: Send 10 credits to Node1:
    // Node0: Request routes:
    let mut routes_0_1 = await!(routes0.request_routes(20,
                           node_public_key(0),
                           node_public_key(1),
                           None)).unwrap();

    assert_eq!(routes_0_1.len(), 1);
    let chosen_route_with_capacity = routes_0_1.pop().unwrap();
    assert_eq!(chosen_route_with_capacity.capacity, 100);
    let chosen_route = chosen_route_with_capacity.route;

    let request_id = Uid::from(&[0x0; UID_LEN]);
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);
    let dest_payment = 10;
    let receipt = await!(send_funds0.request_send_funds(request_id.clone(),
                                            chosen_route,
                                            invoice_id,
                                            dest_payment)).unwrap();
    await!(send_funds0.receipt_ack(request_id, receipt.clone())).unwrap();

    // Node0 allows node1 to have maximum debt of 100 
    // (This should allow to node1 to pay back).
    await!(config0.set_friend_remote_max_debt(node_public_key(1), 100)).unwrap();

    // Allow some time for the index servers to be updated about the new state:
    for _ in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(wspawner.wait()).unwrap();
    }

    // Node1: Send 5 credits to Node0:
    let mut routes_1_0 = await!(routes1.request_routes(10,
                           node_public_key(1),
                           node_public_key(0),
                           None)).unwrap();

    assert_eq!(routes_1_0.len(), 1);
    let chosen_route_with_capacity = routes_1_0.pop().unwrap();
    assert_eq!(chosen_route_with_capacity.capacity, 10);
    let chosen_route = chosen_route_with_capacity.route;

    let request_id = Uid::from(&[0x1; UID_LEN]);
    let invoice_id = InvoiceId::from(&[1; INVOICE_ID_LEN]);
    let dest_payment = 5;
    let receipt = await!(send_funds1.request_send_funds(request_id,
                                            chosen_route.clone(),
                                            invoice_id.clone(),
                                            dest_payment)).unwrap();
    await!(send_funds1.receipt_ack(request_id, receipt.clone())).unwrap();

    // Node1 tries to send credits again: (6 credits):
    // This payment should not work, because we do not have enough trust:
    let request_id = Uid::from(&[0x2; UID_LEN]);
    let invoice_id = InvoiceId::from(&[2; INVOICE_ID_LEN]);
    let dest_payment = 6;
    let res = await!(send_funds1.request_send_funds(request_id,
                                            chosen_route.clone(),
                                            invoice_id,
                                            dest_payment));
    assert!(res.is_err());

}

#[test]
fn test_two_nodes_payment() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_two_nodes_payment(thread_pool.clone()));
}
