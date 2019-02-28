use std::collections::HashMap;
use std::pin::Pin;

use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt, LocalWaker};
use futures::executor::ThreadPool;
use futures::{Future, future, StreamExt, SinkExt, Poll};
use futures_test::future::FutureTestExt;

use tempfile::tempdir;

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
const YIELD_ITERS: usize = 0x1000;


// Based on:
// - https://rust-lang-nursery.github.io/futures-api-docs/0.3.0-alpha.13/src/futures_test/future/pending_once.rs.html#14-17
// - https://github.com/rust-lang-nursery/futures-rs/issues/869
struct Yield(usize);

impl Yield {
    fn new(num_yields: usize) -> Self {
        Yield(num_yields)
    }
}

impl Future for Yield {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, waker: &LocalWaker) -> Poll<Self::Output> {
        let count = &mut self.as_mut().0;
        *count = count.saturating_sub(1);
        if *count == 0 {
            Poll::Ready(())
        } else {
            waker.wake();
            Poll::Pending
        }
    }
}

async fn task_two_nodes_payment<S>(mut spawner: S) 
where
    S: Spawn + Clone + Send + Sync + 'static,
{
    let _ = env_logger::init();
    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();

    // Create a database manager at the temporary directory:
    let sim_db = SimDb::new(temp_dir.path().to_path_buf());

    // A network simulator:
    let sim_net_client = create_sim_network(&mut spawner);

    // Create timer_client:
    let (mut tick_sender, tick_receiver) = mpsc::channel(TIMER_CHANNEL_LEN);
    let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();


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
              spawner.clone()));

    let app0 = await!(create_app(0,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    0,
                    spawner.clone()));


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
              spawner.clone()));

    let app1 = await!(create_app(1,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    1,
                    spawner.clone()));

    // Create relays:
    await!(create_relay(0,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 spawner.clone()));

    await!(create_relay(1,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 spawner.clone()));
    
    // Create three index servers:
    // 0 -- 2 -- 1
    // The only way for information to flow between the two index servers
    // is by having the middle server forward it.
    await!(create_index_server(2,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![0,1],
                             spawner.clone()));


    await!(create_index_server(0,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             spawner.clone()));

    await!(create_index_server(1,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             spawner.clone()));


    let mut config0 = app0.config().unwrap();
    let mut config1 = app1.config().unwrap();

    let mut routes0 = app0.routes().unwrap();
    let mut routes1 = app1.routes().unwrap();

    let mut send_funds0 = app0.send_funds().unwrap();
    let mut send_funds1 = app1.send_funds().unwrap();

    let mut report0 = app0.report();
    let mut report1 = app1.report();

    // Configure relays:
    await!(config0.add_relay(named_relay_address(0))).unwrap();
    await!(config1.add_relay(named_relay_address(1))).unwrap();

    // Configure index servers:
    await!(config0.add_index_server(named_index_server_address(0))).unwrap();
    await!(config1.add_index_server(named_index_server_address(1))).unwrap();

    // Wait some time:
    for i in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(Yield::new(YIELD_ITERS));
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
    for i in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(Yield::new(YIELD_ITERS));
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
            node_report.mutate(&mutation);
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
            node_report.mutate(&mutation);
        }
    }
    drop(mutations_receiver);

    await!(config0.open_friend(node_public_key(1))).unwrap();
    await!(config1.open_friend(node_public_key(0))).unwrap();

    // Wait some time, to let the index servers exchange information:
    for i in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(Yield::new(YIELD_ITERS));
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
    for i in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
        await!(Yield::new(YIELD_ITERS));
    }

    // Node1: Send 5 credits to Node0:
    let mut routes_1_0 = await!(routes0.request_routes(10,
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
