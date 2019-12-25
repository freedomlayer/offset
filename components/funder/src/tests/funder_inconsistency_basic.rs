use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};
use proto::funder::messages::{
    AckClosePayment, AddInvoice, CreatePayment, CreateTransaction, Currency, FriendStatus,
    FriendsRoute, FunderControl, PaymentStatus, RequestResult, RequestsStatus, ResetFriendChannel,
};
use proto::report::messages::{ChannelStatusReport, FunderReport};

use super::utils::{create_node_controls, dummy_named_relay_address, dummy_relay_address};

/// Test a basic inconsistency between two adjacent nodes
async fn task_funder_inconsistency_basic(test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();

    let num_nodes = 2;
    let mut node_controls = create_node_controls(num_nodes, test_executor.clone()).await;

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    let relays0 = vec![dummy_relay_address(0)];
    let relays1 = vec![dummy_relay_address(1)];
    node_controls[0]
        .add_friend(&public_keys[1], relays1.clone(), "node1")
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0, "node0")
        .await;
    assert_eq!(node_controls[0].report.friends.len(), 1);
    assert_eq!(node_controls[1].report.friends.len(), 1);

    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;
    node_controls[1]
        .set_friend_status(&public_keys[0], FriendStatus::Enabled)
        .await;

    // The two nodes should eventually agree to trade `currency1`.
    node_controls[0]
        .set_friend_currencies(&public_keys[1], vec![currency1.clone()])
        .await;
    node_controls[1]
        .set_friend_currencies(&public_keys[0], vec![currency1.clone(), currency2.clone()])
        .await;

    node_controls[0]
        .wait_until_currency_active(&public_keys[1], &currency1)
        .await;
    node_controls[1]
        .wait_until_currency_active(&public_keys[0], &currency1)
        .await;

    // Set remote max debt for both sides:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], &currency1, 200)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], &currency1, 100)
        .await;

    // Open requests:
    node_controls[0]
        .set_requests_status(&public_keys[1], &currency1, RequestsStatus::Open)
        .await;
    node_controls[1]
        .set_requests_status(&public_keys[0], &currency1, RequestsStatus::Open)
        .await;

    // Wait for liveness:
    node_controls[0]
        .wait_until_ready(&public_keys[1], &currency1)
        .await;
    node_controls[1]
        .wait_until_ready(&public_keys[0], &currency1)
        .await;

    // Let node 1 open an invoice:
    let add_invoice = AddInvoice {
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency1.clone(),
        total_dest_payment: 4,
    };
    node_controls[1]
        .send(FunderControl::AddInvoice(add_invoice))
        .await;

    // Create payment 0 --> 1
    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency1.clone(),
        total_dest_payment: 4,
        dest_public_key: node_controls[1].public_key.clone(),
    };
    node_controls[0]
        .send(FunderControl::CreatePayment(create_payment))
        .await;

    // Create transaction 0 --> 1:
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[5u8; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![public_keys[0].clone(), public_keys[1].clone()],
        },
        dest_payment: 4,
        fees: 1,
    };

    node_controls[0]
        .send(FunderControl::CreateTransaction(create_transaction))
        .await;
    let transaction_result = node_controls[0]
        .recv_until_transaction_result()
        .await
        .unwrap();

    let commit = match transaction_result.result {
        RequestResult::Complete(commit) => commit,
        _ => unreachable!(),
    };

    // Commit: 0 ==> 1  (Out of band)

    // 1: Apply Commit:
    node_controls[1]
        .send(FunderControl::CommitInvoice(commit))
        .await;

    // Wait until no more progress can be made
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
    assert_eq!(receipt.dest_payment, 4);
    assert_eq!(receipt.total_dest_payment, 4);

    // Verify expected balances:
    node_controls[0]
        .wait_friend_balance(&public_keys[1], &currency1, -5)
        .await;
    node_controls[1]
        .wait_friend_balance(&public_keys[0], &currency1, 5)
        .await;

    // Wait until no more progress can be made
    test_executor.wait().await;

    ///////////////////////////////////////////////////////////
    // Intentionally create an inconsistency
    ///////////////////////////////////////////////////////////

    // Node0: Remove friend node1, and add it again.
    // This should cause an inconsistency.
    node_controls[0].remove_friend(&public_keys[1]).await;

    node_controls[0]
        .add_friend(&public_keys[1], relays1, "node1")
        .await;

    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
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
        assert!(channel_inconsistent_report.local_reset_terms.is_empty());

        let reset_terms_report =
            if let Some(reset_terms_report) = &channel_inconsistent_report.opt_remote_reset_terms {
                reset_terms_report
            } else {
                return false;
            };

        if let Some(pos) = reset_terms_report
            .balance_for_reset
            .iter()
            .position(|currency_balance| currency_balance.currency == currency1)
        {
            reset_terms_report.balance_for_reset[pos].balance == 5
        } else {
            false
        }
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
    node_controls[0]
        .wait_friend_balance(&public_keys[1], &currency1, -5)
        .await;

    // Wait until channel is consistent with the correct balance:
    node_controls[1]
        .wait_friend_balance(&public_keys[0], &currency1, 5)
        .await;

    // Wait until no more progress can be made
    test_executor.wait().await;

    // Make sure that we manage to send messages over the token channel after resolving the
    // inconsistency:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], &currency1, 400)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], &currency1, 500)
        .await;

    // Wait until no more progress can be made
    test_executor.wait().await;
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
