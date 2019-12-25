use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};
use proto::funder::messages::{
    AckClosePayment, AddInvoice, CreatePayment, CreateTransaction, Currency, FriendStatus,
    FriendsRoute, FunderControl, PaymentStatus, Rate, RequestResult, RequestsStatus,
};

use signature::verify::verify_receipt;

use super::utils::{create_node_controls, dummy_relay_address};

async fn task_funder_basic(test_executor: TestExecutor) {
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
        .add_friend(&public_keys[1], relays1, "node1")
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
        .set_friend_currency_rate(&public_keys[1], &currency1, Rate::new())
        .await;

    // Remove and add currency back, just for testing:
    node_controls[0]
        .remove_friend_currency(&public_keys[1], &currency1)
        .await;
    node_controls[0]
        .set_friend_currency_rate(&public_keys[1], &currency1, Rate::new())
        .await;

    node_controls[1]
        .set_friend_currency_rate(&public_keys[0], &currency1, Rate::new())
        .await;
    node_controls[1]
        .set_friend_currency_rate(&public_keys[0], &currency2, Rate::new())
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
        .wait_until_ready(&public_keys[1])
        .await;
    node_controls[1]
        .wait_until_ready(&public_keys[0])
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

    // We split the payment over two transactions:

    // Create first transaction 0 --> 1: (Pay 3 credits)
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[5u8; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![public_keys[0].clone(), public_keys[1].clone()],
        },
        dest_payment: 3,
        fees: 1,
    };

    node_controls[0]
        .send(FunderControl::CreateTransaction(create_transaction))
        .await;
    let transaction_result = node_controls[0]
        .recv_until_transaction_result()
        .await
        .unwrap();

    // First transaction should be successful, but we don't get a produced
    // commit yet, because we still have 1 more credit to pay:
    match transaction_result.result {
        RequestResult::Success => {}
        _ => unreachable!(),
    };

    // Attempt a second transaction 0 --> 1 with too many credits (It should fail)
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[6u8; Uid::len()]),
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

    // Invoice was fully paid. We get a commit message that we can send out of band:
    match transaction_result.result {
        RequestResult::Failure => {}
        _ => unreachable!(),
    };

    // Create second transaction 0 --> 1: (Pay 1 credit)
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        request_id: Uid::from(&[6u8; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![public_keys[0].clone(), public_keys[1].clone()],
        },
        dest_payment: 1,
        fees: 1,
    };

    node_controls[0]
        .send(FunderControl::CreateTransaction(create_transaction))
        .await;
    let transaction_result = node_controls[0]
        .recv_until_transaction_result()
        .await
        .unwrap();

    // Invoice was fully paid. We get a commit message that we can send out of band:
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

    // We don't know which of the two transactions will be the completed one,
    // because we don't know which one will arrive first.
    vec![1u128, 3u128].contains(&receipt.dest_payment);
    assert_eq!(receipt.total_dest_payment, 4);

    // Verify expected balances:
    node_controls[0]
        .wait_friend_balance(&public_keys[1], &currency1, -6)
        .await;
    node_controls[1]
        .wait_friend_balance(&public_keys[0], &currency1, 6)
        .await;

    // Verify receipt:
    assert!(verify_receipt(&receipt, &public_keys[1]));
}

#[test]
fn test_funder_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_basic(test_executor.clone()));
    assert!(res.is_output());
}
