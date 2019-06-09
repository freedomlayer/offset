use std::collections::HashMap;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::funder::messages::{MultiCommit, PaymentStatus, Rate};

use timer::create_timer_incoming;

use crypto::invoice_id::{InvoiceId, INVOICE_ID_LEN};
use crypto::payment_id::{PaymentId, PAYMENT_ID_LEN};
use crypto::uid::{Uid, UID_LEN};

use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, create_app, create_index_server, create_node, create_relay,
    named_index_server_address, named_relay_address, node_public_key, relay_address, SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

async fn task_nodes_chain(mut test_executor: TestExecutor) {
    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();

    // Create a database manager at the temporary directory:
    let sim_db = SimDb::new(temp_dir.path().to_path_buf());

    // A network simulator:
    let sim_net_client = create_sim_network(&mut test_executor);

    // Create timer_client:
    let (mut tick_sender, tick_receiver) = mpsc::channel(TIMER_CHANNEL_LEN);
    let timer_client = create_timer_incoming(tick_receiver, test_executor.clone()).unwrap();

    let mut apps = Vec::new();

    // Create 6 nodes with apps:
    for i in 0..6 {
        sim_db.init_db(i);

        let mut trusted_apps = HashMap::new();
        trusted_apps.insert(
            i,
            AppPermissions {
                routes: true,
                buyer: true,
                seller: true,
                config: true,
            },
        );

        await!(create_node(
            i,
            sim_db.clone(),
            timer_client.clone(),
            sim_net_client.clone(),
            trusted_apps,
            test_executor.clone()
        ))
        .forget();

        apps.push(
            await!(create_app(
                i,
                sim_net_client.clone(),
                timer_client.clone(),
                i,
                test_executor.clone()
            ))
            .unwrap(),
        );
    }

    // Create relays:
    await!(create_relay(
        0,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone()
    ));

    await!(create_relay(
        1,
        timer_client.clone(),
        sim_net_client.clone(),
        test_executor.clone()
    ));

    // Create three index servers:
    // 0 -- 2 -- 1
    // The only way for information to flow between the two index servers
    // is by having the middle server forward it.
    await!(create_index_server(
        2,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![0, 1],
        test_executor.clone()
    ));

    await!(create_index_server(
        0,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![2],
        test_executor.clone()
    ));

    await!(create_index_server(
        1,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![2],
        test_executor.clone()
    ));

    // Configure relays:

    await!(apps[0].config().unwrap().add_relay(named_relay_address(0))).unwrap();
    await!(apps[0].config().unwrap().add_relay(named_relay_address(1))).unwrap();

    await!(apps[1].config().unwrap().add_relay(named_relay_address(1))).unwrap();
    await!(apps[2].config().unwrap().add_relay(named_relay_address(0))).unwrap();

    await!(apps[3].config().unwrap().add_relay(named_relay_address(1))).unwrap();
    await!(apps[3].config().unwrap().add_relay(named_relay_address(0))).unwrap();

    await!(apps[4].config().unwrap().add_relay(named_relay_address(0))).unwrap();
    await!(apps[5].config().unwrap().add_relay(named_relay_address(1))).unwrap();

    // Configure index servers:
    await!(apps[0]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0)))
    .unwrap();
    await!(apps[0]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2)))
    .unwrap();

    await!(apps[1]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1)))
    .unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2)))
    .unwrap();

    await!(apps[3]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0)))
    .unwrap();

    await!(apps[4]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1)))
    .unwrap();
    await!(apps[4]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0)))
    .unwrap();

    await!(apps[5]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2)))
    .unwrap();
    await!(apps[5]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1)))
    .unwrap();

    // Wait some time:
    // await!(advance_time(40, &mut tick_sender, &test_executor));
    /*
                       5
                       |
             0 -- 1 -- 2 -- 4
                  |
                  3
    */

    // 0 --> 1
    await!(apps[0].config().unwrap().add_friend(
        node_public_key(1),
        vec![relay_address(1)],
        String::from("node1"),
        0
    ))
    .unwrap();
    await!(apps[0].config().unwrap().enable_friend(node_public_key(1))).unwrap();
    await!(apps[0].config().unwrap().open_friend(node_public_key(1))).unwrap();
    await!(apps[0]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(1), 100))
    .unwrap();

    // 1 --> 0
    await!(apps[1].config().unwrap().add_friend(
        node_public_key(0),
        vec![relay_address(1)],
        String::from("node0"),
        0
    ))
    .unwrap();
    await!(apps[1].config().unwrap().enable_friend(node_public_key(0))).unwrap();
    await!(apps[1].config().unwrap().open_friend(node_public_key(0))).unwrap();
    await!(apps[1]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(0), 100))
    .unwrap();
    await!(apps[1]
        .config()
        .unwrap()
        .set_friend_rate(node_public_key(0), Rate { mul: 0, add: 1 }))
    .unwrap();

    // 1 --> 2
    await!(apps[1].config().unwrap().add_friend(
        node_public_key(2),
        vec![relay_address(0)],
        String::from("node2"),
        0
    ))
    .unwrap();
    await!(apps[1].config().unwrap().enable_friend(node_public_key(2))).unwrap();
    await!(apps[1].config().unwrap().open_friend(node_public_key(2))).unwrap();
    await!(apps[1]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(2), 100))
    .unwrap();
    await!(apps[1]
        .config()
        .unwrap()
        .set_friend_rate(node_public_key(2), Rate { mul: 0, add: 2 }))
    .unwrap();

    // 2 --> 1
    await!(apps[2].config().unwrap().add_friend(
        node_public_key(1),
        vec![relay_address(1)],
        String::from("node1"),
        0
    ))
    .unwrap();
    await!(apps[2].config().unwrap().enable_friend(node_public_key(1))).unwrap();
    await!(apps[2].config().unwrap().open_friend(node_public_key(1))).unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(1), 100))
    .unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .set_friend_rate(node_public_key(1), Rate { mul: 0, add: 1 }))
    .unwrap();

    // 1 --> 3
    await!(apps[1].config().unwrap().add_friend(
        node_public_key(3),
        vec![relay_address(0)],
        String::from("node3"),
        0
    ))
    .unwrap();
    await!(apps[1].config().unwrap().enable_friend(node_public_key(3))).unwrap();
    await!(apps[1].config().unwrap().open_friend(node_public_key(3))).unwrap();
    await!(apps[1]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(3), 100))
    .unwrap();

    // 3 --> 1
    await!(apps[3].config().unwrap().add_friend(
        node_public_key(1),
        vec![relay_address(1)],
        String::from("node1"),
        0
    ))
    .unwrap();
    await!(apps[3].config().unwrap().enable_friend(node_public_key(1))).unwrap();
    await!(apps[3].config().unwrap().open_friend(node_public_key(1))).unwrap();
    await!(apps[3]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(1), 100))
    .unwrap();

    // 2 --> 5
    await!(apps[2].config().unwrap().add_friend(
        node_public_key(5),
        vec![relay_address(1)],
        String::from("node5"),
        0
    ))
    .unwrap();
    await!(apps[2].config().unwrap().enable_friend(node_public_key(5))).unwrap();
    await!(apps[2].config().unwrap().open_friend(node_public_key(5))).unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(5), 100))
    .unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .set_friend_rate(node_public_key(5), Rate { mul: 0x80000000, add: 0 }))
    .unwrap();

    // 5 --> 2
    await!(apps[5].config().unwrap().add_friend(
        node_public_key(2),
        vec![relay_address(0)],
        String::from("node2"),
        0
    ))
    .unwrap();
    await!(apps[5].config().unwrap().enable_friend(node_public_key(2))).unwrap();
    await!(apps[5].config().unwrap().open_friend(node_public_key(2))).unwrap();
    await!(apps[5]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(2), 100))
    .unwrap();

    // 2 --> 4
    await!(apps[2].config().unwrap().add_friend(
        node_public_key(4),
        vec![relay_address(0)],
        String::from("node4"),
        0
    ))
    .unwrap();
    await!(apps[2].config().unwrap().enable_friend(node_public_key(4))).unwrap();
    await!(apps[2].config().unwrap().open_friend(node_public_key(4))).unwrap();
    await!(apps[2]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(4), 100))
    .unwrap();

    // 4 --> 2
    // We add the extra relay_address(1) on purpose.
    // Node4 will find out and remove it later.
    await!(apps[4].config().unwrap().add_friend(
        node_public_key(2),
        vec![relay_address(0), relay_address(1)],
        String::from("node2"),
        0
    ))
    .unwrap();
    await!(apps[4].config().unwrap().enable_friend(node_public_key(2))).unwrap();
    await!(apps[4].config().unwrap().open_friend(node_public_key(2))).unwrap();
    await!(apps[4]
        .config()
        .unwrap()
        .set_friend_remote_max_debt(node_public_key(2), 100))
    .unwrap();
    await!(apps[4]
        .config()
        .unwrap()
        .set_friend_rate(node_public_key(2), Rate { mul: 0, add: 1 }))
    .unwrap();

    // Wait some time:
    await!(advance_time(40, &mut tick_sender, &test_executor));

    // Make sure that node1 sees node2 as online:
    let (node_report, mutations_receiver) = await!(apps[1].report().incoming_reports()).unwrap();
    drop(mutations_receiver);
    let friend_report = match node_report.funder_report.friends.get(&node_public_key(2)) {
        None => unreachable!(),
        Some(friend_report) => friend_report,
    };
    assert!(friend_report.liveness.is_online());

    // Perform a payment through the chain: 0 -> 1 -> 2 -> 4
    // ======================================================

    let payment_id = PaymentId::from(&[0u8; PAYMENT_ID_LEN]);
    let invoice_id = InvoiceId::from(&[1u8; INVOICE_ID_LEN]);
    let request_id = Uid::from(&[2u8; UID_LEN]);
    let total_dest_payment = 10u128;
    let fees = 2u128; // Fees for Node1 and Node2

    // Node4: Create an invoice:
    await!(apps[4]
        .seller()
        .unwrap()
        .add_invoice(invoice_id.clone(), total_dest_payment))
    .unwrap();

    // Node0: Request a route to node 4:
    let mut multi_routes = await!(apps[0].routes().unwrap().request_routes(
        20,
        node_public_key(0),
        node_public_key(4),
        None
    ))
    .unwrap();

    assert!(multi_routes.len() > 0);

    let multi_route = multi_routes[0].clone();
    assert_eq!(multi_route.routes.len(), 1);

    let route = multi_route.routes[0].route.clone();

    // Node0: Open a payment to pay the invoice issued by Node4:
    await!(apps[0].buyer().unwrap().create_payment(
        payment_id.clone(),
        invoice_id.clone(),
        total_dest_payment,
        node_public_key(4),
    ))
    .unwrap();

    // Node0: Create one transaction for the given route:
    let commit = await!(apps[0].buyer().unwrap().create_transaction(
        payment_id.clone(),
        request_id.clone(),
        route,
        total_dest_payment,
        fees,
    ))
    .unwrap();

    // Node0: Close payment (No more transactions will be sent through this payment)
    let _ = await!(apps[0]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone()))
    .unwrap();

    // Node0: Compose a MultiCommit:
    let multi_commit = MultiCommit {
        invoice_id: invoice_id.clone(),
        total_dest_payment,
        commits: vec![commit],
    };

    // Node0 now passes the MultiCommit to Node4 out of band.

    // Node4: Apply the MultiCommit
    await!(apps[4].seller().unwrap().commit_invoice(multi_commit)).unwrap();

    // Wait some time:
    await!(advance_time(5, &mut tick_sender, &test_executor));

    // Node0: Check the payment's result:
    let payment_status = await!(apps[0]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone()))
    .unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success((receipt, ack_uid)) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            await!(apps[0]
                .buyer()
                .unwrap()
                .ack_close_payment(payment_id.clone(), ack_uid.clone()))
            .unwrap();
        }
        _ => unreachable!(),
    }

    // Perform a payment through the chain: 5 -> 2 -> 1 -> 3
    // ======================================================

    let payment_id = PaymentId::from(&[3u8; PAYMENT_ID_LEN]);
    let invoice_id = InvoiceId::from(&[4u8; INVOICE_ID_LEN]);
    let request_id = Uid::from(&[5u8; UID_LEN]);
    let total_dest_payment = 10u128;
    let fees = 5u128 + 2u128; // Fees for Node2 (5 = 10/2) and Node1 (2).

    // Node3: Create an invoice:
    await!(apps[3]
        .seller()
        .unwrap()
        .add_invoice(invoice_id.clone(), total_dest_payment))
    .unwrap();

    // Node5: Request a route to node 3:
    let mut multi_routes = await!(apps[5].routes().unwrap().request_routes(
        10,
        node_public_key(5),
        node_public_key(3),
        None
    ))
    .unwrap();

    assert!(multi_routes.len() > 0);

    let multi_route = multi_routes[0].clone();
    assert_eq!(multi_route.routes.len(), 1);

    let route = multi_route.routes[0].route.clone();

    // Node5: Open a payment to pay the invoice issued by Node4:
    await!(apps[5].buyer().unwrap().create_payment(
        payment_id.clone(),
        invoice_id.clone(),
        total_dest_payment,
        node_public_key(3),
    ))
    .unwrap();

    // Node5: Create one transaction for the given route:
    let commit = await!(apps[5].buyer().unwrap().create_transaction(
        payment_id.clone(),
        request_id.clone(),
        route,
        total_dest_payment,
        fees,
    ))
    .unwrap();

    // Node5: Close payment (No more transactions will be sent through this payment)
    let _ = await!(apps[5]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone()))
    .unwrap();

    // Node5: Compose a MultiCommit:
    let multi_commit = MultiCommit {
        invoice_id: invoice_id.clone(),
        total_dest_payment,
        commits: vec![commit],
    };

    // Node5 now passes the MultiCommit to Node3 out of band.

    // Node3: Apply the MultiCommit
    await!(apps[3].seller().unwrap().commit_invoice(multi_commit)).unwrap();

    // Wait some time:
    await!(advance_time(5, &mut tick_sender, &test_executor));

    // Node5: Check the payment's result:
    let payment_status = await!(apps[5]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone()))
    .unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success((receipt, ack_uid)) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            await!(apps[5]
                .buyer()
                .unwrap()
                .ack_close_payment(payment_id.clone(), ack_uid.clone()))
            .unwrap();
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_nodes_chain() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_nodes_chain(test_executor.clone()));
    assert!(res.is_output());
}
