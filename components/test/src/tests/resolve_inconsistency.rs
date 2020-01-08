use std::collections::HashMap;
use std::convert::TryFrom;

use futures::channel::mpsc;

use tempfile::tempdir;

use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};
use proto::funder::messages::{Currency, FriendsRoute, PaymentStatus, PaymentStatusSuccess, Rate};
use proto::report::messages::ChannelStatusReport;

use app::conn::{self, ConnPairApp, RequestResult};

use timer::create_timer_incoming;

use crate::node_report_service::node_report_service;
use crate::utils::{
    advance_time, create_app, create_node, create_relay, named_relay_address, node_public_key,
    relay_address, SimDb,
};

use crate::sim_network::create_sim_network;

use crate::app_wrapper::{
    ack_close_payment, create_transaction, request_close_payment, send_request,
};

const TIMER_CHANNEL_LEN: usize = 0;

/// Perform a basic payment between a buyer and a seller.
/// Use a trivial route of [buyer, seller] instead of using an index server.
/// Node0 sends credits to Node1
pub async fn make_test_payment(
    mut conn_pair0: &mut ConnPairApp, // buyer
    mut conn_pair1: &mut ConnPairApp, // seller
    buyer_public_key: PublicKey,
    seller_public_key: PublicKey,
    currency: Currency,
    total_dest_payment: u128,
    fees: u128,
    mut tick_sender: mpsc::Sender<()>,
    test_executor: TestExecutor,
) -> PaymentStatus {
    let payment_id = PaymentId::from(&[4u8; PaymentId::len()]);
    let invoice_id = InvoiceId::from(&[3u8; InvoiceId::len()]);
    let request_id = Uid::from(&[5u8; Uid::len()]);

    send_request(
        &mut conn_pair1,
        conn::seller::add_invoice(invoice_id.clone(), currency.clone(), total_dest_payment),
    )
    .await
    .unwrap();

    // Note: We build the route on our own, without using index servers:
    let route = FriendsRoute {
        public_keys: vec![buyer_public_key.clone(), seller_public_key.clone()],
    };

    // Node0: Open a payment to pay the invoice issued by Node1:
    send_request(
        &mut conn_pair0,
        conn::buyer::create_payment(
            payment_id.clone(),
            invoice_id.clone(),
            currency.clone(),
            total_dest_payment,
            seller_public_key.clone(),
        ),
    )
    .await
    .unwrap();

    // Node0: Create one transaction for the given route:
    let request_result = create_transaction(
        conn_pair0,
        payment_id.clone(),
        request_id.clone(),
        route.clone(),
        total_dest_payment,
        fees,
    )
    .await
    .unwrap();

    let commit = if let RequestResult::Complete(commit) = request_result {
        commit
    } else {
        unreachable!();
    };

    // Node0 now passes the Commit to Node1 out of band.

    // Node1: Apply the Commit
    send_request(&mut conn_pair1, conn::seller::commit_invoice(commit))
        .await
        .unwrap();

    // Node0: Close payment (No more transactions will be sent through this payment)
    let _ = request_close_payment(conn_pair0, payment_id.clone())
        .await
        .unwrap();

    // Wait some time:
    advance_time(5, &mut tick_sender, &test_executor).await;

    // Node0: Check the payment's result:
    let payment_status = request_close_payment(conn_pair0, payment_id.clone())
        .await
        .unwrap();

    // Acknowledge the payment closing result if required:
    match &payment_status {
        PaymentStatus::Success(PaymentStatusSuccess { receipt, ack_uid }) => {
            assert_eq!(receipt.total_dest_payment, total_dest_payment);
            assert_eq!(receipt.invoice_id, invoice_id);
            ack_close_payment(conn_pair0, payment_id.clone(), ack_uid.clone())
                .await
                .unwrap();
        }
        PaymentStatus::Canceled(ack_uid) => {
            ack_close_payment(conn_pair0, payment_id.clone(), ack_uid.clone())
                .await
                .unwrap();
        }
        _ => unreachable!(),
    }

    payment_status
}

async fn task_resolve_inconsistency(mut test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();

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

    /*
    let mut config0 = app0.config().unwrap().clone();
    let mut config1 = app1.config().unwrap().clone();

    let mut buyer0 = app0.buyer().unwrap().clone();
    let mut seller1 = app1.seller().unwrap().clone();

    let mut report0 = app0.report().clone();
    let mut report1 = app1.report().clone();
    */

    let (_permissions0, node_report0, conn_pair0) = app0;
    let (_permissions1, node_report1, conn_pair1) = app1;

    let (sender0, receiver0) = conn_pair0.split();
    let (receiver0, mut report_client0) =
        node_report_service(node_report0, receiver0, &test_executor);
    let mut conn_pair0 = ConnPairApp::from_raw(sender0, receiver0);

    let (sender1, receiver1) = conn_pair1.split();
    let (receiver1, mut report_client1) =
        node_report_service(node_report1, receiver1, &test_executor);
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
    advance_time(40, &mut tick_sender, &test_executor).await;

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

    send_request(
        &mut conn_pair0,
        conn::config::enable_friend(node_public_key(1)),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::enable_friend(node_public_key(0)),
    )
    .await
    .unwrap();

    // Set active currencies for both sides:
    send_request(
        &mut conn_pair0,
        conn::config::set_friend_currency_rate(node_public_key(1), currency1.clone(), Rate::new()),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::set_friend_currency_rate(node_public_key(0), currency1.clone(), Rate::new()),
    )
    .await
    .unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    send_request(
        &mut conn_pair0,
        conn::config::set_friend_currency_max_debt(node_public_key(1), currency1.clone(), 100),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::set_friend_currency_max_debt(node_public_key(0), currency1.clone(), 200),
    )
    .await
    .unwrap();

    send_request(
        &mut conn_pair0,
        conn::config::open_friend_currency(node_public_key(1), currency1.clone()),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::open_friend_currency(node_public_key(0), currency1.clone()),
    )
    .await
    .unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Send 10 currency1 credits from node0 to node1:
    let payment_status = make_test_payment(
        &mut conn_pair0,
        &mut conn_pair1,
        node_public_key(0),
        node_public_key(1),
        currency1.clone(),
        8u128, // total_dest_payment
        2u128, // fees
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    if let PaymentStatus::Success(_) = payment_status {
    } else {
        unreachable!();
    };

    // Node0: remove the friend node1:
    send_request(
        &mut conn_pair0,
        conn::config::remove_friend(node_public_key(1)),
    )
    .await
    .unwrap();

    // Node0: Add node1 as a friend with a different balance
    // This should cause an inconsistency when the first token
    // message will be sent.
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

    // Node0 enables the friend node1. This is required to trigger the inconsistency error
    send_request(
        &mut conn_pair0,
        conn::config::enable_friend(node_public_key(1)),
    )
    .await
    .unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node1 should now perceive the mutual channel with node0 to be inconsistent:
    let incon_report = loop {
        let node_report = report_client1.request_report().await;
        let opt_friend_report = node_report.funder_report.friends.get(&node_public_key(0));

        let friend_report = if let Some(friend_report) = opt_friend_report {
            friend_report
        } else {
            continue;
        };

        match &friend_report.channel_status {
            ChannelStatusReport::Consistent(_) => continue,
            ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
                if channel_inconsistent_report.opt_remote_reset_terms.is_some() {
                    break channel_inconsistent_report.clone();
                } else {
                    continue;
                }
            }
        };
    };

    assert_eq!(incon_report.local_reset_terms[0].currency, currency1);
    assert_eq!(incon_report.local_reset_terms[0].balance, 10);

    let remote_reset_terms = incon_report.opt_remote_reset_terms.clone().unwrap();
    assert_eq!(remote_reset_terms.balance_for_reset.len(), 0);

    // Node2 should perceive the mutual channel with node1 to be inconsistent:
    let incon_report = loop {
        let node_report = report_client0.request_report().await;
        let opt_friend_report = node_report.funder_report.friends.get(&node_public_key(1));

        let friend_report = if let Some(friend_report) = opt_friend_report {
            friend_report
        } else {
            continue;
        };

        match &friend_report.channel_status {
            ChannelStatusReport::Consistent(_) => continue,
            ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
                if channel_inconsistent_report.opt_remote_reset_terms.is_some() {
                    break channel_inconsistent_report.clone();
                } else {
                    continue;
                }
            }
        };
    };
    assert_eq!(incon_report.local_reset_terms.len(), 0);

    let remote_reset_terms = incon_report.opt_remote_reset_terms.clone().unwrap();
    assert_eq!(remote_reset_terms.balance_for_reset[0].currency, currency1);
    assert_eq!(remote_reset_terms.balance_for_reset[0].balance, 10);

    let reset_token = remote_reset_terms.reset_token.clone();

    // Node0 agrees to the conditions of Node1:
    send_request(
        &mut conn_pair0,
        conn::config::reset_friend_channel(node_public_key(1), reset_token),
    )
    .await
    .unwrap();

    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Channel should be consistent now:
    loop {
        let node_report = report_client0.request_report().await;
        let opt_friend_report = node_report.funder_report.friends.get(&node_public_key(1));

        let friend_report = if let Some(friend_report) = opt_friend_report {
            friend_report
        } else {
            continue;
        };

        match &friend_report.channel_status {
            ChannelStatusReport::Consistent(_) => break,
            ChannelStatusReport::Inconsistent(_) => continue,
        };
    }

    // Node1: Channel should be consistent now:
    loop {
        let node_report = report_client1.request_report().await;
        let opt_friend_report = node_report.funder_report.friends.get(&node_public_key(0));

        let friend_report = if let Some(friend_report) = opt_friend_report {
            friend_report
        } else {
            continue;
        };

        match &friend_report.channel_status {
            ChannelStatusReport::Consistent(_) => break,
            ChannelStatusReport::Inconsistent(_) => continue,
        };
    }

    // Let both sides open the channel:
    send_request(
        &mut conn_pair0,
        conn::config::open_friend_currency(node_public_key(1), currency1.clone()),
    )
    .await
    .unwrap();
    send_request(
        &mut conn_pair1,
        conn::config::open_friend_currency(node_public_key(0), currency1.clone()),
    )
    .await
    .unwrap();

    advance_time(20, &mut tick_sender, &test_executor).await;

    /*
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
    */
}

#[test]
fn test_resolve_inconsistency() {
    // let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_resolve_inconsistency(test_executor.clone()));
    assert!(res.is_output());
}
