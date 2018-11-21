use futures::executor::ThreadPool;
use futures::task::Spawn;

use crypto::identity::PublicKey;
use crypto::uid::{Uid, UID_LEN};

use crate::types::{IncomingControlMessage, FriendStatus,
    RequestsStatus, FriendsRoute, UserRequestSendFunds,
    InvoiceId, INVOICE_ID_LEN, ResponseSendFundsResult,
    ReceiptAck};
use crate::report::{FunderReport,
                    ChannelStatusReport};
use super::utils::create_node_controls;



async fn task_funder_basic(spawner: impl Spawn + Clone + Send + 'static) {
    let num_nodes = 2;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    await!(node_controls[0].add_friend(&public_keys[1], 1u32, "node1", 8));
    await!(node_controls[1].add_friend(&public_keys[0], 0u32, "node0", -8));
    assert_eq!(node_controls[0].report.friends.len(), 1);
    assert_eq!(node_controls[1].report.friends.len(), 1);

    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enable));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enable));

    // Set remote max debt for both sides:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 100));

    // Open requests:
    await!(node_controls[0].set_requests_status(&public_keys[1], RequestsStatus::Open));
    await!(node_controls[1].set_requests_status(&public_keys[0], RequestsStatus::Open));

    // Wait for liveness:
    await!(node_controls[0].wait_until_alive(&public_keys[1]));
    await!(node_controls[1].wait_until_alive(&public_keys[0]));

    // Send credits 0 --> 1
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![
            node_controls[0].public_key.clone(), 
            node_controls[1].public_key.clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 5,
    };
    await!(node_controls[0].send(IncomingControlMessage::RequestSendFunds(user_request_send_funds))).unwrap();
    let response_received = await!(node_controls[0].recv_until_response()).unwrap();

    let pred = |report: &FunderReport<_>| report.num_ready_receipts == 1;
    await!(node_controls[0].recv_until(pred));

    assert_eq!(response_received.request_id, Uid::from(&[3; UID_LEN]));
    let receipt = match response_received.result {
        ResponseSendFundsResult::Failure(_) => unreachable!(),
        ResponseSendFundsResult::Success(send_funds_receipt) => send_funds_receipt,
    };

    let receipt_ack = ReceiptAck {
        request_id: Uid::from(&[3; UID_LEN]),
        receipt_signature: receipt.signature.clone(),
    };
    await!(node_controls[0].send(IncomingControlMessage::ReceiptAck(receipt_ack))).unwrap();

    let pred = |report: &FunderReport<_>| report.num_ready_receipts == 0;
    await!(node_controls[0].recv_until(pred));

    // Verify expected balances:
    let friend = node_controls[0].report.friends.get(&public_keys[1]).unwrap();
    let tc_report = match &friend.channel_status {
       ChannelStatusReport::Consistent(tc_report) => tc_report,
       _ => unreachable!(),
    };
    assert_eq!(tc_report.balance.balance, 3);


    let pred = |report: &FunderReport<_>| {
       let friend = report.friends.get(&public_keys[0]).unwrap();
       let tc_report = match &friend.channel_status {
           ChannelStatusReport::Consistent(tc_report) => tc_report,
           _ => unreachable!(),
       };
       tc_report.balance.balance == -3
    };
    await!(node_controls[1].recv_until(pred));
}

#[test]
fn test_funder_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_basic(thread_pool.clone()));
}


async fn task_funder_forward_payment(spawner: impl Spawn + Clone + Send + 'static) {
    /*
     * 0 -- 1 -- 2
     */
    let num_nodes = 3;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    // Create topology:
    // ----------------
    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    // Add friends:
    await!(node_controls[0].add_friend(&public_keys[1], 1u32, "node1", 8));
    await!(node_controls[1].add_friend(&public_keys[0], 0u32, "node0", -8));
    await!(node_controls[1].add_friend(&public_keys[2], 2u32, "node2", 6));
    await!(node_controls[2].add_friend(&public_keys[1], 0u32, "node0", -6));

    // Enable friends:
    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enable));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enable));
    await!(node_controls[1].set_friend_status(&public_keys[2], FriendStatus::Enable));
    await!(node_controls[2].set_friend_status(&public_keys[1], FriendStatus::Enable));

    // Set remote max debt:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 100));
    await!(node_controls[1].set_remote_max_debt(&public_keys[2], 300));
    await!(node_controls[2].set_remote_max_debt(&public_keys[1], 400));

    // Open requests, allowing this route: 0 --> 1 --> 2
    await!(node_controls[1].set_requests_status(&public_keys[0], RequestsStatus::Open));
    await!(node_controls[2].set_requests_status(&public_keys[1], RequestsStatus::Open));

    // Wait until alive:
    await!(node_controls[0].wait_until_alive(&public_keys[1]));
    await!(node_controls[1].wait_until_alive(&public_keys[0]));
    await!(node_controls[1].wait_until_alive(&public_keys[2]));
    await!(node_controls[2].wait_until_alive(&public_keys[1]));


    // Send credits 0 --> 2
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![
            public_keys[0].clone(), 
            public_keys[1].clone(), 
            public_keys[2].clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 20,
    };
    await!(node_controls[0].send(IncomingControlMessage::RequestSendFunds(user_request_send_funds))).unwrap();
    let response_received = await!(node_controls[0].recv_until_response()).unwrap();
    assert_eq!(response_received.request_id, Uid::from(&[3; UID_LEN]));
    let receipt = match response_received.result {
        ResponseSendFundsResult::Failure(_) => unreachable!(),
        ResponseSendFundsResult::Success(send_funds_receipt) => send_funds_receipt,
    };

    // Send ReceiptAck:
    let receipt_ack = ReceiptAck {
        request_id: Uid::from(&[3; UID_LEN]),
        receipt_signature: receipt.signature.clone(),
    };
    await!(node_controls[0].send(IncomingControlMessage::ReceiptAck(receipt_ack))).unwrap();

    let pred = |report: &FunderReport<_>| report.num_ready_receipts == 0;
    await!(node_controls[0].recv_until(pred));

    // Make sure that node2 got the credits:
    let pred = |report: &FunderReport<_>| {
        let friend = match report.friends.get(&public_keys[1]) {
            None => return false,
            Some(friend) => friend,
        };
        let tc_report = match &friend.channel_status {
           ChannelStatusReport::Consistent(tc_report) => tc_report,
           _ => return false,
        };
        tc_report.balance.balance == -6 + 20
    };
    await!(node_controls[2].recv_until(pred));
}

#[test]
fn test_funder_forward_payment() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_forward_payment(thread_pool.clone()));
}
