use futures::executor::ThreadPool;
use futures::task::Spawn;

use crypto::identity::PublicKey;
use crypto::uid::{Uid, UID_LEN};

use proto::funder::messages::{FriendsRoute, InvoiceId, INVOICE_ID_LEN,
                            FunderIncomingControl, FriendStatus,
                            RequestsStatus, UserRequestSendFunds,
                            ReceiptAck, ResetFriendChannel,
                            ResponseSendFundsResult};
use proto::funder::report::{FunderReport,
                    ChannelStatusReport};

use super::utils::create_node_controls;



async fn task_funder_basic(spawner: impl Spawn + Clone + Send + 'static) {
    let num_nodes = 2;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    println!("Was here1");

    await!(node_controls[0].add_friend(&public_keys[1], 1u32, "node1", 8));
    await!(node_controls[1].add_friend(&public_keys[0], 0u32, "node0", -8));
    println!("Was here2");
    assert_eq!(node_controls[0].report.friends.len(), 1);
    assert_eq!(node_controls[1].report.friends.len(), 1);

    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enabled));
    println!("Was here3");

    // Set remote max debt for both sides:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 100));

    println!("Was here4");
    // Open requests:
    await!(node_controls[0].set_requests_status(&public_keys[1], RequestsStatus::Open));
    await!(node_controls[1].set_requests_status(&public_keys[0], RequestsStatus::Open));

    // Wait for liveness:
    await!(node_controls[0].wait_until_ready(&public_keys[1]));
    await!(node_controls[1].wait_until_ready(&public_keys[0]));


    // Send credits 0 --> 1
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![
            node_controls[0].public_key.clone(), 
            node_controls[1].public_key.clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 5,
    };
    await!(node_controls[0].send(FunderIncomingControl::RequestSendFunds(user_request_send_funds))).unwrap();
    let response_received = await!(node_controls[0].recv_until_response()).unwrap();

    assert_eq!(response_received.request_id, Uid::from(&[3; UID_LEN]));
    let receipt = match response_received.result {
        ResponseSendFundsResult::Failure(_) => unreachable!(),
        ResponseSendFundsResult::Success(send_funds_receipt) => send_funds_receipt,
    };

    let receipt_ack = ReceiptAck {
        request_id: Uid::from(&[3; UID_LEN]),
        receipt_signature: receipt.signature.clone(),
    };
    await!(node_controls[0].send(FunderIncomingControl::ReceiptAck(receipt_ack))).unwrap();

    let pred = |report: &FunderReport<_>| report.num_ready_receipts == 0;
    await!(node_controls[0].recv_until(pred));

    // Verify expected balances:
    let pred = |report: &FunderReport<_>| {
       let friend = report.friends.get(&public_keys[1]).unwrap();
       let tc_report = match &friend.channel_status {
           ChannelStatusReport::Consistent(tc_report) => tc_report,
           _ => return false,
       };
       tc_report.balance.balance == 3
    };
    await!(node_controls[0].recv_until(pred));


    let pred = |report: &FunderReport<_>| {
       let friend = report.friends.get(&public_keys[0]).unwrap();
       let tc_report = match &friend.channel_status {
           ChannelStatusReport::Consistent(tc_report) => tc_report,
           _ => return false,
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
    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[2], FriendStatus::Enabled));
    await!(node_controls[2].set_friend_status(&public_keys[1], FriendStatus::Enabled));

    // Set remote max debt:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 100));
    await!(node_controls[1].set_remote_max_debt(&public_keys[2], 300));
    await!(node_controls[2].set_remote_max_debt(&public_keys[1], 400));

    // Open requests, allowing this route: 0 --> 1 --> 2
    await!(node_controls[1].set_requests_status(&public_keys[0], RequestsStatus::Open));
    await!(node_controls[2].set_requests_status(&public_keys[1], RequestsStatus::Open));

    // Wait until route is ready (Online + Consistent + open requests)
    // Note: We don't need the other direction to be ready, because the request is sent
    // along the following route: 0 --> 1 --> 2
    await!(node_controls[0].wait_until_ready(&public_keys[1]));
    await!(node_controls[1].wait_until_ready(&public_keys[2]));

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
    await!(node_controls[0].send(FunderIncomingControl::RequestSendFunds(user_request_send_funds))).unwrap();
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
    await!(node_controls[0].send(FunderIncomingControl::ReceiptAck(receipt_ack))).unwrap();

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

async fn task_funder_payment_failure(spawner: impl Spawn + Clone + Send + 'static) {
    /*
     * 0 -- 1 -- 2
     * We will try to send payment from 0 along the route 0 -- 1 -- 2 -- 3,
     * where 3 does not exist. We expect that node 2 will return a failure response.
     */
    let num_nodes = 4;
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
    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[2], FriendStatus::Enabled));
    await!(node_controls[2].set_friend_status(&public_keys[1], FriendStatus::Enabled));

    // Set remote max debt:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 100));
    await!(node_controls[1].set_remote_max_debt(&public_keys[2], 300));
    await!(node_controls[2].set_remote_max_debt(&public_keys[1], 400));

    // Open requests, allowing this route: 0 --> 1 --> 2
    await!(node_controls[1].set_requests_status(&public_keys[0], RequestsStatus::Open));
    await!(node_controls[2].set_requests_status(&public_keys[1], RequestsStatus::Open));

    // Wait until route is ready (Online + Consistent + open requests)
    // Note: We don't need the other direction to be ready, because the request is sent
    // along the following route: 0 --> 1 --> 2
    await!(node_controls[0].wait_until_ready(&public_keys[1]));
    await!(node_controls[1].wait_until_ready(&public_keys[2]));


    // Send credits 0 --> 2
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![
            public_keys[0].clone(), 
            public_keys[1].clone(), 
            public_keys[2].clone(),
            public_keys[3].clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 20,
    };
    await!(node_controls[0].send(FunderIncomingControl::RequestSendFunds(user_request_send_funds))).unwrap();
    let response_received = await!(node_controls[0].recv_until_response()).unwrap();
    assert_eq!(response_received.request_id, Uid::from(&[3; UID_LEN]));
    let reporting_public_key = match response_received.result {
        ResponseSendFundsResult::Failure(reporting_public_key) => reporting_public_key,
        ResponseSendFundsResult::Success(_) => unreachable!(),
    };

    assert_eq!(reporting_public_key, public_keys[2]);

    let friend = node_controls[2].report.friends.get(&public_keys[1]).unwrap();
    let tc_report = match &friend.channel_status {
       ChannelStatusReport::Consistent(tc_report) => tc_report,
       _ => unreachable!(),
    };
    assert_eq!(tc_report.balance.balance, -6);
}

#[test]
fn test_funder_payment_failure() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_payment_failure(thread_pool.clone()));
}

/// Test a basic inconsistency between two adjacent nodes
async fn task_funder_inconsistency_basic<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let num_nodes = 2;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    // We set incompatible initial balances (non zero sum) to cause an inconsistency:
    await!(node_controls[0].add_friend(&public_keys[1], 1u32, "node1", 20));
    await!(node_controls[1].add_friend(&public_keys[0], 0u32, "node0", -8));

    await!(node_controls[0].set_friend_status(&public_keys[1], FriendStatus::Enabled));
    await!(node_controls[1].set_friend_status(&public_keys[0], FriendStatus::Enabled));

    // Expect inconsistency, together with reset terms:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[1]).unwrap();
        let channel_inconsistent_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(_) => return false,
            ChannelStatusReport::Inconsistent(channel_inconsistent_report) => 
                channel_inconsistent_report
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
    await!(node_controls[0].recv_until(pred));

    // Resolve inconsistency
    // ---------------------

    // Obtain reset terms:
    let friend = node_controls[0].report.friends.get(&public_keys[1]).unwrap();
    let channel_inconsistent_report = match &friend.channel_status {
        ChannelStatusReport::Consistent(_) => unreachable!(),
        ChannelStatusReport::Inconsistent(channel_inconsistent_report) => 
            channel_inconsistent_report
    };
    let reset_terms_report = match &channel_inconsistent_report.opt_remote_reset_terms {
        Some(reset_terms_report) => reset_terms_report,
        None => unreachable!(),
    };

    let reset_friend_channel = ResetFriendChannel {
        friend_public_key: public_keys[1].clone(),
        current_token: reset_terms_report.reset_token.clone(), // TODO: Rename current_token to reset_token?
    };
    await!(node_controls[0].send(FunderIncomingControl::ResetFriendChannel(reset_friend_channel))).unwrap();

    // Wait until channel is consistent with the correct balance:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[1]).unwrap();
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(tc_report) => tc_report,
            ChannelStatusReport::Inconsistent(_) => return false,
        };
        tc_report.balance.balance == 8
    };
    await!(node_controls[0].recv_until(pred));

    // Wait until channel is consistent with the correct balance:
    let pred = |report: &FunderReport<_>| {
        let friend = report.friends.get(&public_keys[0]).unwrap();
        let tc_report = match &friend.channel_status {
            ChannelStatusReport::Consistent(tc_report) => tc_report,
            ChannelStatusReport::Inconsistent(_) => return false,
        };
        tc_report.balance.balance == -8
    };
    await!(node_controls[1].recv_until(pred));

    // Make sure that we manage to send messages over the token channel after resolving the
    // inconsistency:
    await!(node_controls[0].set_remote_max_debt(&public_keys[1], 200));
    await!(node_controls[1].set_remote_max_debt(&public_keys[0], 300));
}

#[test]
fn test_funder_inconsistency_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_inconsistency_basic(thread_pool.clone()));
}

/// Test setting relay address for local node
async fn task_funder_set_address(spawner: impl Spawn + Clone + Send + 'static) {
    let num_nodes = 1;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    // Change the node's relay address:
    await!(node_controls[0].set_address(9876u32));
}


#[test]
fn test_funder_set_address() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_set_address(thread_pool.clone()));
}

