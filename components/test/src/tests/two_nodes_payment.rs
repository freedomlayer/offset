use std::collections::HashMap;

use futures::channel::mpsc;
use futures::StreamExt;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::funder::messages::{FriendsRoute, MultiCommit, PaymentStatus, PaymentStatusSuccess};

use timer::create_timer_incoming;

use crypto::rand::CryptoRandom;

use proto::crypto::{
    InvoiceId, PaymentId, PublicKey, Uid, INVOICE_ID_LEN, PAYMENT_ID_LEN, UID_LEN,
};

use app::{AppBuyer, AppRoutes, AppSeller};

use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, create_app, create_index_server, create_node, create_relay,
    named_index_server_address, named_relay_address, node_public_key, relay_address, SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

/// Perform a basic payment between a buyer and a seller.
async fn make_test_payment<'a, R>(
    app_buyer: &'a mut AppBuyer<R>,
    app_seller: &'a mut AppSeller<R>,
    app_routes: &'a mut AppRoutes<R>,
    buyer_public_key: PublicKey,
    seller_public_key: PublicKey,
    total_dest_payment: u128,
    fees: u128,
    mut tick_sender: mpsc::Sender<()>,
    test_executor: TestExecutor,
) -> PaymentStatus
where
    R: CryptoRandom,
{
    let payment_id = PaymentId::from(&[4u8; PAYMENT_ID_LEN]);
    let invoice_id = InvoiceId::from(&[3u8; INVOICE_ID_LEN]);
    let request_id = Uid::from(&[5u8; UID_LEN]);

    await!(app_seller.add_invoice(invoice_id.clone(), total_dest_payment)).unwrap();

    // Node0: Request routes:
    let mut routes = await!(app_routes.request_routes(
        total_dest_payment.checked_add(fees).unwrap(),
        buyer_public_key.clone(),
        seller_public_key.clone(),
        None
    ))
    .unwrap();

    let multi_route = routes.pop().unwrap();
    let route = multi_route.routes[0].clone();

    // Node0: Open a payment to pay the invoice issued by Node1:
    await!(app_buyer.create_payment(
        payment_id.clone(),
        invoice_id.clone(),
        total_dest_payment,
        seller_public_key.clone()
    ))
    .unwrap();

    // Node0: Create one transaction for the given route:
    let commit = await!(app_buyer.create_transaction(
        payment_id.clone(),
        request_id.clone(),
        route.route.clone(),
        total_dest_payment,
        fees,
    ))
    .unwrap();

    // Node0: Close payment (No more transactions will be sent through this payment)
    let _ = await!(app_buyer.request_close_payment(payment_id.clone())).unwrap();

    // Node0: Compose a MultiCommit:
    let multi_commit = MultiCommit {
        invoice_id: invoice_id.clone(),
        total_dest_payment,
        commits: vec![commit],
    };

    // Node0 now passes the MultiCommit to Node1 out of band.

    // Node1: Apply the MultiCommit
    await!(app_seller.commit_invoice(multi_commit)).unwrap();

    // Wait some time:
    await!(advance_time(5, &mut tick_sender, &test_executor));

    // Node0: Check the payment's result:
    let payment_status = await!(app_buyer.request_close_payment(payment_id.clone())).unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success(PaymentStatusSuccess { receipt, ack_uid }) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            await!(app_buyer.ack_close_payment(payment_id.clone(), ack_uid.clone())).unwrap();
        }
        PaymentStatus::Canceled(ack_uid) => {
            await!(app_buyer.ack_close_payment(payment_id.clone(), ack_uid.clone())).unwrap();
        }
        _ => unreachable!(),
    }

    payment_status
}

async fn task_two_nodes_payment(mut test_executor: TestExecutor) {
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

    await!(create_node(
        0,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone()
    ))
    .forget();

    // Connection attempt to the wrong node should fail:
    let opt_wrong_app = await!(create_app(
        0,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone()
    ));
    assert!(opt_wrong_app.is_none());

    let mut app0 = await!(create_app(
        0,
        sim_net_client.clone(),
        timer_client.clone(),
        0,
        test_executor.clone()
    ))
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
    await!(create_node(
        1,
        sim_db.clone(),
        timer_client.clone(),
        sim_net_client.clone(),
        trusted_apps,
        test_executor.clone()
    ))
    .forget();

    let mut app1 = await!(create_app(
        1,
        sim_net_client.clone(),
        timer_client.clone(),
        1,
        test_executor.clone()
    ))
    .unwrap();

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

    let mut config0 = app0.config().unwrap().clone();
    let mut config1 = app1.config().unwrap().clone();

    let mut routes0 = app0.routes().unwrap().clone();
    let mut routes1 = app1.routes().unwrap().clone();

    let mut buyer0 = app0.buyer().unwrap().clone();
    let mut seller0 = app0.seller().unwrap().clone();
    let mut buyer1 = app1.buyer().unwrap().clone();
    let mut seller1 = app1.seller().unwrap().clone();

    let mut report0 = app0.report().clone();
    let mut report1 = app1.report().clone();

    // Configure relays:
    await!(config0.add_relay(named_relay_address(0))).unwrap();
    await!(config1.add_relay(named_relay_address(1))).unwrap();

    // Configure index servers:
    await!(config0.add_index_server(named_index_server_address(0))).unwrap();
    await!(config1.add_index_server(named_index_server_address(1))).unwrap();

    // Wait some time:
    await!(advance_time(40, &mut tick_sender, &test_executor));

    // Node0: Add node1 as a friend:
    await!(config0.add_friend(
        node_public_key(1),
        vec![relay_address(1)],
        String::from("node1"),
        100
    ))
    .unwrap();

    // Node1: Add node0 as a friend:
    await!(config1.add_friend(
        node_public_key(0),
        vec![relay_address(0)],
        String::from("node0"),
        -100
    ))
    .unwrap();

    // Node0: Enable/Disable/Enable node1:
    await!(config0.enable_friend(node_public_key(1))).unwrap();
    await!(advance_time(10, &mut tick_sender, &test_executor));
    await!(config0.disable_friend(node_public_key(1))).unwrap();
    await!(advance_time(10, &mut tick_sender, &test_executor));
    await!(config0.enable_friend(node_public_key(1))).unwrap();
    await!(advance_time(10, &mut tick_sender, &test_executor));

    // Node1: Enable node0:
    await!(config1.enable_friend(node_public_key(0))).unwrap();

    await!(advance_time(40, &mut tick_sender, &test_executor));

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
    await!(advance_time(40, &mut tick_sender, &test_executor));

    let payment_status = await!(make_test_payment(
        &mut buyer0,
        &mut seller1,
        &mut routes0,
        node_public_key(0),
        node_public_key(1),
        8u128, // total_dest_payment
        2u128, // fees
        tick_sender.clone(),
        test_executor.clone()
    ));

    if let PaymentStatus::Success(_) = payment_status {
    } else {
        unreachable!();
    };

    // Node0 allows node1 to have maximum debt of 100
    // (This should allow to node1 to pay back).
    await!(config0.set_friend_remote_max_debt(node_public_key(1), 100)).unwrap();

    // Allow some time for the index servers to be updated about the new state:
    await!(advance_time(40, &mut tick_sender, &test_executor));

    // Node1: Send 5 = 3 + 2 credits to Node0:
    let payment_status = await!(make_test_payment(
        &mut buyer1,
        &mut seller0,
        &mut routes1,
        node_public_key(1),
        node_public_key(0),
        3u128, // total_dest_payment
        2u128, // fees
        tick_sender.clone(),
        test_executor.clone()
    ));

    if let PaymentStatus::Success(_) = payment_status {
    } else {
        unreachable!();
    };

    // Node1: Attempt to send 6 more credits.
    // This should not work, because 6 + 5 = 11 > 8.
    let total_dest_payment = 6;
    let fees = 0;
    let payment_id = PaymentId::from(&[8u8; PAYMENT_ID_LEN]);
    let invoice_id = InvoiceId::from(&[9u8; INVOICE_ID_LEN]);
    let request_id = Uid::from(&[10u8; UID_LEN]);

    await!(seller0.add_invoice(invoice_id.clone(), total_dest_payment)).unwrap();

    // Node1: Open a payment to pay the invoice issued by Node1:
    await!(buyer1.create_payment(
        payment_id.clone(),
        invoice_id.clone(),
        total_dest_payment,
        node_public_key(0),
    ))
    .unwrap();

    // Use the route (pk1, pk0)
    let route = FriendsRoute {
        public_keys: vec![node_public_key(1), node_public_key(0)],
    };
    // Node1: Create one transaction for the given route:
    let res = await!(buyer1.create_transaction(
        payment_id.clone(),
        request_id.clone(),
        route,
        total_dest_payment,
        fees,
    ));
    assert!(res.is_err());

    // Node0: Check the payment's result:
    let payment_status = await!(buyer1.request_close_payment(payment_id.clone())).unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Canceled(ack_uid) => {
            await!(buyer1.ack_close_payment(payment_id.clone(), ack_uid.clone())).unwrap();
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_two_nodes_payment() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_two_nodes_payment(test_executor.clone()));
    assert!(res.is_output());
}
