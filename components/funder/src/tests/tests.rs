use common::test_executor::TestExecutor;

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};
use proto::funder::messages::{
    AckClosePayment, AddInvoice, CreatePayment, CreateTransaction, FriendStatus, FriendsRoute,
    FunderControl, MultiCommit, PaymentStatus, Rate, RequestResult, RequestsStatus,
    ResetFriendChannel,
};
use proto::report::messages::{ChannelStatusReport, FunderReport};

use super::utils::{create_node_controls, dummy_named_relay_address, dummy_relay_address};


async fn task_funder_forward_payment(test_executor: TestExecutor) {
    /*
     * 0 -- 1 -- 2
     */
    let num_nodes = 3;
    let mut node_controls = create_node_controls(num_nodes, test_executor.clone()).await;

    // Create topology:
    // ----------------
    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    // Add friends:
    let relays0 = vec![dummy_relay_address(0)];
    let relays1 = vec![dummy_relay_address(1)];
    let relays2 = vec![dummy_relay_address(2)];
    node_controls[0]
        .add_friend(&public_keys[1], relays1, "node1", 8)
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0.clone(), "node0", -8)
        .await;
    node_controls[1]
        .add_friend(&public_keys[2], relays2, "node2", 6)
        .await;
    node_controls[2]
        .add_friend(&public_keys[1], relays0, "node0", -6)
        .await;

    // Enable friends:
    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[0], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[2], FriendStatus::Enabled)
        .await;
    node_controls[2]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;

    // Set rate:
    // This is the amount of credits node 1 takes from node 0 for forwarding messages.
    node_controls[1]
        .set_friend_rate(&public_keys[0], Rate { mul: 0, add: 5 })
        .await;

    // Set remote max debt:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], 200)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], 100)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[2], 300)
        .await;
    node_controls[2]
        .set_remote_max_debt(&public_keys[1], 400)
        .await;

    // Open requests, allowing this route: 0 --> 1 --> 2
    node_controls[1]
        .set_requests_status(&public_keys[0], RequestsStatus::Open)
        .await;
    node_controls[2]
        .set_requests_status(&public_keys[1], RequestsStatus::Open)
        .await;

    // Wait until route is ready (Online + Consistent + open requests)
    // Note: We don't need the other direction to be ready, because the request is sent
    // along the following route: 0 --> 1 --> 2
    node_controls[0].wait_until_ready(&public_keys[1]).await;
    node_controls[1].wait_until_ready(&public_keys[2]).await;

    // Let node 2 open an invoice:
    let add_invoice = AddInvoice {
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        total_dest_payment: 15,
    };
    node_controls[2]
        .send(FunderControl::AddInvoice(add_invoice))
        .await;

    // Create payment 0 --> 2
    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        total_dest_payment: 15,
        dest_public_key: node_controls[2].public_key.clone(),
    };
    node_controls[0]
        .send(FunderControl::CreatePayment(create_payment))
        .await;

    // Create transaction 0 --> 2:
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[5u8; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![
                public_keys[0].clone(),
                public_keys[1].clone(),
                public_keys[2].clone(),
            ],
        },
        dest_payment: 15,
        fees: 5,
    };
    node_controls[0]
        .send(FunderControl::CreateTransaction(create_transaction))
        .await;
    let transaction_result = node_controls[0]
        .recv_until_transaction_result()
        .await
        .unwrap();

    let commit = match transaction_result.result {
        RequestResult::Success(commit) => commit,
        _ => unreachable!(),
    };

    // 0: Create multi commit:
    let multi_commit = MultiCommit {
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        total_dest_payment: 15,
        commits: vec![commit],
    };

    // MultiCommit: 0 ==> 2  (Out of band)

    // 2: Apply MultiCommit:
    node_controls[2]
        .send(FunderControl::CommitInvoice(multi_commit))
        .await;

    // Wait until no more progress can be made (We should get a receipt)
    test_executor.wait().await;

    // 0: Expect a receipt:

    node_controls[0]
        .send(FunderControl::RequestClosePayment(PaymentId::from(
            &[2u8; PaymentId::len()],
        )))
        .await;
    let response_close_payment = node_controls[0]
        .recv_until_response_close_payment()
        .await
        .unwrap();
    let (receipt, ack_uid) = match response_close_payment.status {
        PaymentStatus::Success(payment_status_success) => (
            payment_status_success.receipt,
            payment_status_success.ack_uid,
        ),
        _ => unreachable!(),
    };

    // 0: Acknowledge response close:
    let ack_close_payment = AckClosePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        ack_uid,
    };
    node_controls[0]
        .send(FunderControl::AckClosePayment(ack_close_payment))
        .await;

    assert_eq!(
        receipt.invoice_id,
        InvoiceId::from(&[1u8; InvoiceId::len()])
    );
    assert_eq!(receipt.dest_payment, 15);
    assert_eq!(receipt.total_dest_payment, 15);

    // Make sure that node2 got the credits:
    let pred = |report: &FunderReport<_>| {
        let friend = match report.friends.get(&public_keys[1]) {
            None => return false,
            Some(friend) => friend,
        };
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            _ => return false,
        };
        tc_report.balance.balance == (-6 + 15)
    };
    node_controls[2].recv_until(pred).await;

    // Make sure that node1 got his fees:
    let pred = |report: &FunderReport<_>| {
        // Balance with node 0:
        let friend = match report.friends.get(&public_keys[0]) {
            None => return false,
            Some(friend) => friend,
        };
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            _ => return false,
        };

        if tc_report.balance.balance != (-8 + 20) {
            return false;
        }

        // Balance with node 2:
        let friend = match report.friends.get(&public_keys[2]) {
            None => return false,
            Some(friend) => friend,
        };
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            _ => return false,
        };
        tc_report.balance.balance == (6 - 15)
    };
    node_controls[1].recv_until(pred).await;
}

#[test]
fn test_funder_forward_payment() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_forward_payment(test_executor.clone()));
    assert!(res.is_output());
}

async fn task_funder_payment_failure(test_executor: TestExecutor) {
    /*
     * 0 -- 1 -- 2
     * We will try to send payment from 0 along the route 0 -- 1 -- 2 -- 3,
     * where 3 does not exist. We expect that node 2 will return a failure response.
     */
    let num_nodes = 4;
    let mut node_controls = create_node_controls(num_nodes, test_executor.clone()).await;

    // Create topology:
    // ----------------
    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    // Add friends:
    let relays0 = vec![dummy_relay_address(0)];
    let relays1 = vec![dummy_relay_address(1)];
    let relays2 = vec![dummy_relay_address(2)];
    node_controls[0]
        .add_friend(&public_keys[1], relays1, "node1", 8)
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0.clone(), "node0", -8)
        .await;
    node_controls[1]
        .add_friend(&public_keys[2], relays2, "node2", 6)
        .await;
    node_controls[2]
        .add_friend(&public_keys[1], relays0, "node0", -6)
        .await;

    // Enable friends:
    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[0], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[2], FriendStatus::Enabled)
        .await;
    node_controls[2]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;

    // Set rate:
    // This is the amount of credits node 1 takes from node 0 for forwarding messages.
    node_controls[1]
        .set_friend_rate(&public_keys[0], Rate { mul: 0, add: 5 })
        .await;

    // Set remote max debt:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], 200)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], 100)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[2], 300)
        .await;
    node_controls[2]
        .set_remote_max_debt(&public_keys[1], 400)
        .await;

    // Open requests, allowing this route: 0 --> 1 --> 2
    node_controls[1]
        .set_requests_status(&public_keys[0], RequestsStatus::Open)
        .await;
    node_controls[2]
        .set_requests_status(&public_keys[1], RequestsStatus::Open)
        .await;

    // Wait until route is ready (Online + Consistent + open requests)
    // Note: We don't need the other direction to be ready, because the request is sent
    // along the following route: 0 --> 1 --> 2
    node_controls[0].wait_until_ready(&public_keys[1]).await;
    node_controls[1].wait_until_ready(&public_keys[2]).await;

    // Create payment 0 --> 3 (Where 3 does not exist)
    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        total_dest_payment: 15,
        dest_public_key: node_controls[3].public_key.clone(),
    };
    node_controls[0]
        .send(FunderControl::CreatePayment(create_payment))
        .await;

    // Create transaction 0 --> 3 (3 does not exist):
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[5u8; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![
                public_keys[0].clone(),
                public_keys[1].clone(),
                public_keys[2].clone(),
                public_keys[3].clone(),
            ],
        },
        dest_payment: 15,
        fees: 5,
    };
    node_controls[0]
        .send(FunderControl::CreateTransaction(create_transaction))
        .await;
    let transaction_result = node_controls[0]
        .recv_until_transaction_result()
        .await
        .unwrap();

    // We expect failure:
    match transaction_result.result {
        RequestResult::Failure => {}
        _ => unreachable!(),
    }

    // 0: Expect that the payment was canceled:
    let ack_uid = loop {
        node_controls[0]
            .send(FunderControl::RequestClosePayment(PaymentId::from(
                &[2u8; PaymentId::len()],
            )))
            .await;
        let response_close_payment = node_controls[0]
            .recv_until_response_close_payment()
            .await
            .unwrap();
        match response_close_payment.status {
            PaymentStatus::Canceled(ack_uid) => break ack_uid,
            _ => {}
        }
    };

    // 0: Acknowledge response close:
    let ack_close_payment = AckClosePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        ack_uid,
    };
    node_controls[0]
        .send(FunderControl::AckClosePayment(ack_close_payment))
        .await;

    // Make sure that node0's balance is left unchanged:
    let pred = |report: &FunderReport<_>| {
        let friend = match report.friends.get(&public_keys[1]) {
            None => return false,
            Some(friend) => friend,
        };
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            _ => return false,
        };
        tc_report.balance.balance == 8
    };
    node_controls[0].recv_until(pred).await;
}

#[test]
fn test_funder_payment_failure() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_payment_failure(test_executor.clone()));
    assert!(res.is_output());
}

/// Test a basic inconsistency between two adjacent nodes
async fn task_funder_inconsistency_basic(test_executor: TestExecutor) {
    let num_nodes = 2;
    let mut node_controls = create_node_controls(num_nodes, test_executor).await;

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    // We set incompatible initial balances (non zero sum) to cause an inconsistency:
    let relays0 = vec![dummy_relay_address(0)];
    let relays1 = vec![dummy_relay_address(1)];
    node_controls[0]
        .add_friend(&public_keys[1], relays1, "node1", 20)
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0, "node0", -8)
        .await;

    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[0], FriendStatus::Enabled)
        .await;

    // Expect inconsistency, together with reset terms:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[1]).unwrap();
        let channel_inconsistent_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(_) => return false,
            ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
                channel_inconsistent_report
            }
        };
        if channel_inconsistent_report.local_reset_terms_balance != 20 {
            return false;
        }
        let reset_terms_report = match &channel_inconsistent_report.opt_remote_reset_terms {
            None => return false,
            Some(reset_terms_report) => reset_terms_report,
        };
        reset_terms_report.balance_for_reset == -8
    };
    node_controls[0].recv_until(pred).await;

    // Resolve inconsistency
    // ---------------------

    // Obtain reset terms:
    let friend = node_controls[0]
        .report
        .friends
        .get(&public_keys[1])
        .unwrap();
    let channel_inconsistent_report = match &friend.channel_status {
        ChannelStatusReport::Consistent(_) => unreachable!(),
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
            channel_inconsistent_report
        }
    };
    let reset_terms_report = match &channel_inconsistent_report.opt_remote_reset_terms {
        Some(reset_terms_report) => reset_terms_report,
        None => unreachable!(),
    };

    let reset_friend_channel = ResetFriendChannel {
        friend_public_key: public_keys[1].clone(),
        reset_token: reset_terms_report.reset_token.clone(), // TODO: Rename reset_token to reset_token?
    };
    node_controls[0]
        .send(FunderControl::ResetFriendChannel(reset_friend_channel))
        .await;

    // Wait until channel is consistent with the correct balance:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[1]).unwrap();
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            ChannelStatusReport::Inconsistent(_) => return false,
        };
        tc_report.balance.balance == 8
    };
    node_controls[0].recv_until(pred).await;

    // Wait until channel is consistent with the correct balance:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[0]).unwrap();
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(channel_consistent) => &channel_consistent.tc_report,
            ChannelStatusReport::Inconsistent(_) => return false,
        };
        tc_report.balance.balance == -8
    };
    node_controls[1].recv_until(pred).await;

    // Make sure that we manage to send messages over the token channel after resolving the
    // inconsistency:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], 200)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], 300)
        .await;
}

#[test]
fn test_funder_inconsistency_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_inconsistency_basic(test_executor.clone()));
    assert!(res.is_output());
}

/// Test setting relay address for local node
async fn task_funder_add_relay(test_executor: TestExecutor) {
    let num_nodes = 1;
    let mut node_controls = create_node_controls(num_nodes, test_executor).await;

    // Change the node's relay address:
    let named_relay = dummy_named_relay_address(5);
    // Add twice. Second addition should have no effect:
    node_controls[0].add_relay(named_relay.clone()).await;
    node_controls[0].add_relay(named_relay.clone()).await;
    // Remove relay:
    node_controls[0].remove_relay(named_relay.public_key).await;
}

#[test]
fn test_funder_add_relay() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_add_relay(test_executor.clone()));
    assert!(res.is_output());
}

// TODO: Add a test for multi-route payment
