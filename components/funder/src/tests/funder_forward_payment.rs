use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};
use proto::funder::messages::{
    AckClosePayment, AddInvoice, CreatePayment, CreateTransaction, Currency, FriendStatus,
    FriendsRoute, FunderControl, PaymentStatus, Rate, RequestResult, RequestsStatus,
};

use super::utils::{create_node_controls, dummy_relay_address};

async fn task_funder_forward_payment(test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();

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
        .add_friend(&public_keys[1], relays1, "node1")
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0.clone(), "node0")
        .await;
    node_controls[1]
        .add_friend(&public_keys[2], relays2, "node2")
        .await;
    node_controls[2]
        .add_friend(&public_keys[1], relays0, "node0")
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

    test_executor.wait().await;

    // Add active currencies:
    node_controls[0]
        .set_friend_currencies(&public_keys[1], vec![currency1.clone()])
        .await;
    node_controls[1]
        .set_friend_currencies(&public_keys[0], vec![currency1.clone(), currency2.clone()])
        .await;
    node_controls[1]
        .set_friend_currencies(&public_keys[2], vec![currency1.clone(), currency2.clone()])
        .await;
    node_controls[2]
        .set_friend_currencies(&public_keys[1], vec![currency1.clone(), currency2.clone()])
        .await;

    test_executor.wait().await;

    // Wait for active currencies to be ready:
    node_controls[0]
        .wait_until_currency_active(&public_keys[1], &currency1)
        .await;
    node_controls[1]
        .wait_until_currency_active(&public_keys[0], &currency1)
        .await;
    node_controls[1]
        .wait_until_currency_active(&public_keys[2], &currency1)
        .await;
    node_controls[2]
        .wait_until_currency_active(&public_keys[1], &currency1)
        .await;
    node_controls[1]
        .wait_until_currency_active(&public_keys[2], &currency2)
        .await;
    node_controls[2]
        .wait_until_currency_active(&public_keys[1], &currency2)
        .await;

    test_executor.wait().await;
    // Set rate:
    // This is the amount of credits node 1 takes from node 0 for forwarding messages.
    node_controls[1]
        .set_friend_currency_rate(&public_keys[0], &currency1, Rate { mul: 0, add: 5 })
        .await;

    // Set remote max debt:
    node_controls[0]
        .set_remote_max_debt(&public_keys[1], &currency1, 200)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[0], &currency1, 100)
        .await;
    node_controls[1]
        .set_remote_max_debt(&public_keys[2], &currency1, 300)
        .await;
    node_controls[2]
        .set_remote_max_debt(&public_keys[1], &currency1, 400)
        .await;

    // Open requests, allowing this route: 0 --> 1 --> 2 for currency1:
    node_controls[1]
        .set_requests_status(&public_keys[0], &currency1, RequestsStatus::Open)
        .await;
    node_controls[2]
        .set_requests_status(&public_keys[1], &currency1, RequestsStatus::Open)
        .await;

    // Just for testing sake, also add 2 --> 1 for currency2:
    node_controls[2]
        .set_requests_status(&public_keys[1], &currency2, RequestsStatus::Open)
        .await;

    // This will fail, because node1 and node0 don't trade in currency2:
    node_controls[1]
        .set_requests_status(&public_keys[0], &currency2, RequestsStatus::Open)
        .await;

    // Wait until route is ready (Online + Consistent + open requests)
    // Note: We don't need the other direction to be ready, because the request is sent
    // along the following route: 0 --> 1 --> 2
    node_controls[0]
        .wait_until_ready(&public_keys[1], &currency1)
        .await;
    node_controls[1]
        .wait_until_ready(&public_keys[2], &currency1)
        .await;

    // Let node 2 open an invoice:
    let add_invoice = AddInvoice {
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency1.clone(),
        total_dest_payment: 15,
    };
    node_controls[2]
        .send(FunderControl::AddInvoice(add_invoice))
        .await;

    // Create payment 0 --> 2
    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[2u8; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency1.clone(),
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
        RequestResult::Complete(commit) => commit,
        _ => unreachable!(),
    };


    // Commit: 0 ==> 2  (Out of band)

    // 2: Apply Commit:
    node_controls[2]
        .send(FunderControl::CommitInvoice(commit))
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

    // Wait until no more progress can be made (All payments should have already happened):
    test_executor.wait().await;

    // Make sure that node2 got the credits:
    node_controls[2]
        .wait_friend_balance(&public_keys[1], &currency1, 15)
        .await;

    // Make sure that node1 got his fees:
    node_controls[1]
        .wait_friend_balance(&public_keys[0], &currency1, 20)
        .await;

    node_controls[1]
        .wait_friend_balance(&public_keys[2], &currency1, -15)
        .await;

    // Verify balance from the side of node0:
    node_controls[0]
        .wait_friend_balance(&public_keys[1], &currency1, -20)
        .await;
}

#[test]
fn test_funder_forward_payment() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_forward_payment(test_executor.clone()));
    assert!(res.is_output());
}
