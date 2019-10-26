use std::collections::HashMap;
use std::convert::TryFrom;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::funder::messages::{Currency, PaymentStatus, PaymentStatusSuccess, Rate};

use timer::create_timer_incoming;

use proto::crypto::{InvoiceId, PaymentId, Uid};

use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, create_app, create_index_server, create_node, create_relay,
    named_index_server_address, named_relay_address, node_public_key, relay_address, SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

async fn task_nodes_chain(mut test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();

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

        create_node(
            i,
            sim_db.clone(),
            timer_client.clone(),
            sim_net_client.clone(),
            trusted_apps,
            test_executor.clone(),
        )
        .await
        .forget();

        apps.push(
            create_app(
                i,
                sim_net_client.clone(),
                timer_client.clone(),
                i,
                test_executor.clone(),
            )
            .await
            .unwrap(),
        );
    }

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

    // Create three index servers:
    // 0 -- 2 -- 1
    // The only way for information to flow between the two index servers
    // is by having the middle server forward it.
    create_index_server(
        2,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![0, 1],
        test_executor.clone(),
    )
    .await;

    create_index_server(
        0,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![2],
        test_executor.clone(),
    )
    .await;

    create_index_server(
        1,
        timer_client.clone(),
        sim_net_client.clone(),
        vec![2],
        test_executor.clone(),
    )
    .await;

    // Configure relays:
    let mut node_relays: Vec<Vec<u8>> = Vec::new();
    node_relays.push(vec![0, 1]); // node 0
    node_relays.push(vec![1]); // node 1
    node_relays.push(vec![0]); // node 2
    node_relays.push(vec![0, 1]); // node 3
    node_relays.push(vec![0]); // node 4
    node_relays.push(vec![1]); // node 5

    // Change into immutable:
    let node_relays = node_relays;

    for (node, relays) in node_relays.iter().enumerate() {
        for relay in relays {
            apps[node]
                .config()
                .unwrap()
                .add_relay(named_relay_address(*relay))
                .await
                .unwrap();
        }
    }

    // Configure index servers:
    apps[0]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0))
        .await
        .unwrap();
    apps[0]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2))
        .await
        .unwrap();

    apps[1]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1))
        .await
        .unwrap();
    apps[2]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2))
        .await
        .unwrap();

    apps[3]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0))
        .await
        .unwrap();

    apps[4]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1))
        .await
        .unwrap();
    apps[4]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(0))
        .await
        .unwrap();

    apps[5]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(2))
        .await
        .unwrap();
    apps[5]
        .config()
        .unwrap()
        .add_index_server(named_index_server_address(1))
        .await
        .unwrap();

    // Wait some time:
    // advance_time(40, &mut tick_sender, &test_executor).await;
    /*
                       5
                       |
             0 -- 1 -- 2 -- 4
                  |
                  3
    */

    // Enable friends and set active currencies:
    // 0 -- 1
    for (i, j) in &[(0u8, 1u8), (1, 3), (1, 2), (2, 5), (2, 4)] {
        for (&a, &b) in &[(i, j), (j, i)] {
            apps[a as usize]
                .config()
                .unwrap()
                .add_friend(
                    node_public_key(b),
                    vec![relay_address(node_relays[b as usize][0])], // Pick one relay
                    format!("node{}", b),
                )
                .await
                .unwrap();
            apps[a as usize]
                .config()
                .unwrap()
                .enable_friend(node_public_key(b))
                .await
                .unwrap();

            for currency in [&currency1, &currency2].iter() {
                apps[a as usize]
                    .config()
                    .unwrap()
                    .set_friend_currency_rate(node_public_key(b), (*currency).clone(), Rate::new())
                    // .set_friend_currencies(node_public_key(b), vec![currency1.clone(), currency2.clone()])
                    .await
                    .unwrap();
            }
        }
    }

    // Wait until active currencies are negotiated:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Open channels:
    for (i, j) in &[(0u8, 1u8), (1, 3), (1, 2), (2, 5), (2, 4)] {
        for (&a, &b) in &[(i, j), (j, i)] {
            apps[a as usize]
                .config()
                .unwrap()
                .set_friend_currency_max_debt(node_public_key(b), currency1.clone(), 100)
                .await
                .unwrap();
        }
    }
    for (i, j) in &[(0u8, 1u8), (1, 3), (1, 2), (2, 5), (2, 4)] {
        for (&a, &b) in &[(i, j), (j, i)] {
            apps[a as usize]
                .config()
                .unwrap()
                .open_friend_currency(node_public_key(b), currency1.clone())
                .await
                .unwrap();
        }
    }

    // 0 --> 1

    // 1 --> 0
    apps[1]
        .config()
        .unwrap()
        .set_friend_currency_rate(
            node_public_key(0),
            currency1.clone(),
            Rate { mul: 0, add: 1 },
        )
        .await
        .unwrap();

    // 1 --> 2
    apps[1]
        .config()
        .unwrap()
        .set_friend_currency_rate(
            node_public_key(2),
            currency1.clone(),
            Rate { mul: 0, add: 2 },
        )
        .await
        .unwrap();

    // 2 --> 1
    apps[2]
        .config()
        .unwrap()
        .set_friend_currency_rate(
            node_public_key(1),
            currency1.clone(),
            Rate { mul: 0, add: 1 },
        )
        .await
        .unwrap();

    // 1 --> 3

    // 3 --> 1

    // 2 --> 5
    apps[2]
        .config()
        .unwrap()
        .set_friend_currency_rate(
            node_public_key(5),
            currency1.clone(),
            Rate {
                mul: 0x80000000,
                add: 0,
            },
        )
        .await
        .unwrap();

    // 5 --> 2

    // 2 --> 4

    // 4 --> 2
    apps[4]
        .config()
        .unwrap()
        .set_friend_currency_rate(
            node_public_key(2),
            currency1.clone(),
            Rate { mul: 0, add: 1 },
        )
        .await
        .unwrap();

    // Wait some time:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Make sure that nodes see each other as online along the chain 0 -- 1 -- 2 -- 4:
    for (i, j) in &[(0u8, 1u8), (1, 2), (2, 4)] {
        for (&a, &b) in &[(i, j), (j, i)] {
            let (node_report, mutations_receiver) =
                apps[a as usize].report().incoming_reports().await.unwrap();
            drop(mutations_receiver);
            let friend_report = match node_report.funder_report.friends.get(&node_public_key(b)) {
                None => unreachable!(),
                Some(friend_report) => friend_report,
            };
            assert!(friend_report.liveness.is_online());
        }
    }

    // Perform a payment through the chain: 0 -> 1 -> 2 -> 4
    // ======================================================

    let payment_id = PaymentId::from(&[0u8; PaymentId::len()]);
    let invoice_id = InvoiceId::from(&[1u8; InvoiceId::len()]);
    let request_id = Uid::from(&[2u8; Uid::len()]);
    let total_dest_payment = 10u128;
    let fees = 2u128; // Fees for Node1 and Node2

    // Node4: Create an invoice:
    apps[4]
        .seller()
        .unwrap()
        .add_invoice(invoice_id.clone(), currency1.clone(), total_dest_payment)
        .await
        .unwrap();

    // Node0: Request a route to node 4:
    let multi_routes = apps[0]
        .routes()
        .unwrap()
        .request_routes(
            currency1.clone(),
            20,
            node_public_key(0),
            node_public_key(4),
            None,
        )
        .await
        .unwrap();

    assert!(multi_routes.len() > 0);

    let multi_route = multi_routes[0].clone();
    assert_eq!(multi_route.routes.len(), 1);

    let route = multi_route.routes[0].route.clone();

    // Node0: Open a payment to pay the invoice issued by Node4:
    apps[0]
        .buyer()
        .unwrap()
        .create_payment(
            payment_id.clone(),
            invoice_id.clone(),
            currency1.clone(),
            total_dest_payment,
            node_public_key(4),
        )
        .await
        .unwrap();

    // Node0: Create one transaction for the given route:
    let commit = apps[0]
        .buyer()
        .unwrap()
        .create_transaction(
            payment_id.clone(),
            request_id.clone(),
            route,
            total_dest_payment,
            fees,
        )
        .await
        .unwrap()
        .unwrap();

    // Node0: Close payment (No more transactions will be sent through this payment)
    let _ = apps[0]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone())
        .await
        .unwrap();

    // Node0 now passes the Commit to Node4 out of band.

    // Node4: Apply the Commit
    apps[4]
        .seller()
        .unwrap()
        .commit_invoice(commit)
        .await
        .unwrap();

    // Wait some time:
    advance_time(5, &mut tick_sender, &test_executor).await;

    // Node0: Check the payment's result:
    let payment_status = apps[0]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone())
        .await
        .unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success(PaymentStatusSuccess { receipt, ack_uid }) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            apps[0]
                .buyer()
                .unwrap()
                .ack_close_payment(payment_id.clone(), ack_uid.clone())
                .await
                .unwrap();
        }
        _ => unreachable!(),
    }

    // Perform a payment through the chain: 5 -> 2 -> 1 -> 3
    // ======================================================

    let payment_id = PaymentId::from(&[3u8; PaymentId::len()]);
    let invoice_id = InvoiceId::from(&[4u8; InvoiceId::len()]);
    let request_id = Uid::from(&[5u8; Uid::len()]);
    let total_dest_payment = 10u128;
    let fees = 5u128 + 2u128; // Fees for Node2 (5 = 10/2) and Node1 (2).

    // Node3: Create an invoice:
    apps[3]
        .seller()
        .unwrap()
        .add_invoice(invoice_id.clone(), currency1.clone(), total_dest_payment)
        .await
        .unwrap();

    // Node5: Request a route to node 3:
    let multi_routes = apps[5]
        .routes()
        .unwrap()
        .request_routes(
            currency1.clone(),
            10,
            node_public_key(5),
            node_public_key(3),
            None,
        )
        .await
        .unwrap();

    assert!(multi_routes.len() > 0);

    let multi_route = multi_routes[0].clone();
    assert_eq!(multi_route.routes.len(), 1);

    let route = multi_route.routes[0].route.clone();

    // Node5: Open a payment to pay the invoice issued by Node4:
    apps[5]
        .buyer()
        .unwrap()
        .create_payment(
            payment_id.clone(),
            invoice_id.clone(),
            currency1.clone(),
            total_dest_payment,
            node_public_key(3),
        )
        .await
        .unwrap();

    // Node5: Create one transaction for the given route:
    let commit = apps[5]
        .buyer()
        .unwrap()
        .create_transaction(
            payment_id.clone(),
            request_id.clone(),
            route,
            total_dest_payment,
            fees,
        )
        .await
        .unwrap()
        .unwrap();

    // Node5: Close payment (No more transactions will be sent through this payment)
    let _ = apps[5]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone())
        .await
        .unwrap();

    // Node5 now passes the Commit to Node3 out of band.

    // Node3: Apply the Commit
    apps[3]
        .seller()
        .unwrap()
        .commit_invoice(commit)
        .await
        .unwrap();

    // Wait some time:
    advance_time(5, &mut tick_sender, &test_executor).await;

    // Node5: Check the payment's result:
    let payment_status = apps[5]
        .buyer()
        .unwrap()
        .request_close_payment(payment_id.clone())
        .await
        .unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success(PaymentStatusSuccess { receipt, ack_uid }) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            apps[5]
                .buyer()
                .unwrap()
                .ack_close_payment(payment_id.clone(), ack_uid.clone())
                .await
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
