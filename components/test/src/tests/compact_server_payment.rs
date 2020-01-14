use std::collections::HashMap;
use std::convert::TryFrom;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use tempfile::tempdir;

use common::conn::ConnPair;
use common::test_executor::TestExecutor;

use proto::app_server::messages::AppPermissions;
use proto::crypto::{InvoiceId, PaymentId, PublicKey};
use proto::funder::messages::{Currency, Rate, Receipt};

use timer::create_timer_incoming;

use app::gen::gen_uid;

use stcompact::compact_node::messages::{
    AddFriend, AddInvoice, CompactToUser, CompactToUserAck, ConfirmPaymentFees,
    FriendLivenessReport, InitPayment, OpenFriendCurrency, PaymentDoneStatus, PaymentFeesResponse,
    RequestVerifyCommit, SetFriendCurrencyMaxDebt, SetFriendCurrencyRate, UserToCompact,
    UserToCompactAck, VerifyCommitStatus,
};
use stcompact::messages::{
    CreateNode, CreateNodeLocal, NodeName, ServerToUser, ServerToUserAck, UserToServer,
    UserToServerAck,
};

use crate::compact_server_wrapper::send_request;
use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, create_compact_node, create_compact_server, create_index_server, create_node,
    create_relay, named_index_server_address, named_relay_address, node_public_key, relay_address,
    SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

/*
use crate::compact_report_service::compact_report_service;


/// Perform a basic payment between a buyer and a seller.
/// Node0 sends credits to Node1
async fn make_test_payment(
    mut conn_pair0: &mut ConnPair<UserToCompactAck, CompactToUserAck>,
    mut conn_pair1: &mut ConnPair<UserToCompactAck, CompactToUserAck>,
    _buyer_public_key: PublicKey,
    seller_public_key: PublicKey,
    currency: Currency,
    total_dest_payment: u128,
    mut tick_sender: mpsc::Sender<()>,
    test_executor: TestExecutor,
) -> Option<(Receipt, u128)> {
    let payment_id = PaymentId::from(&[4u8; PaymentId::len()]);
    let invoice_id = InvoiceId::from(&[3u8; InvoiceId::len()]);
    // let request_id = Uid::from(&[5u8; Uid::len()]);

    // Node1: Add invoice:
    let add_invoice = AddInvoice {
        invoice_id: invoice_id.clone(),
        currency: currency.clone(),
        total_dest_payment,
        description: "Example payment".to_owned(),
    };
    send_request(&mut conn_pair1, UserToCompact::AddInvoice(add_invoice))
        .await
        .unwrap();

    // Node0: Init payment
    let init_payment = InitPayment {
        payment_id: payment_id.clone(),
        invoice_id: invoice_id.clone(),
        currency: currency.clone(),
        dest_public_key: seller_public_key.clone(),
        dest_payment: total_dest_payment,
        description: "Example payment".to_owned(),
    };
    send_request(&mut conn_pair0, UserToCompact::InitPayment(init_payment))
        .await
        .unwrap();

    // Node0: Wait for payment fees:
    let (_fees, confirm_id) = loop {
        let compact_to_user_ack = conn_pair0.receiver.next().await.unwrap();
        let payment_fees =
            if let CompactToUserAck::CompactToUser(CompactToUser::PaymentFees(payment_fees)) =
                compact_to_user_ack
            {
                payment_fees
            } else {
                continue;
            };
        assert_eq!(payment_fees.payment_id, payment_id);
        match payment_fees.response {
            PaymentFeesResponse::Fees(fees, confirm_id) => break (fees, confirm_id),
            _ => unreachable!(),
        };
    };

    // Node0: Confirm payment fees:
    let confirm_payment_fees = ConfirmPaymentFees {
        payment_id: payment_id.clone(),
        confirm_id,
    };
    send_request(
        &mut conn_pair0,
        UserToCompact::ConfirmPaymentFees(confirm_payment_fees),
    )
    .await
    .unwrap();

    // Node0: Wait for commit to be created:
    let commit = loop {
        let compact_to_user_ack = conn_pair0.receiver.next().await.unwrap();
        let payment_commit = match compact_to_user_ack {
            CompactToUserAck::CompactToUser(CompactToUser::PaymentCommit(payment_commit)) => {
                payment_commit
            }
            CompactToUserAck::CompactToUser(CompactToUser::PaymentDone(payment_done)) => {
                // We get here if the remote side rejects our request for payment:
                assert_eq!(payment_done.payment_id, payment_id);
                match payment_done.status {
                    PaymentDoneStatus::Failure(ack_uid) => {
                        // Node0: AckPaymentDone:
                        send_request(
                            &mut conn_pair0,
                            UserToCompact::AckPaymentDone(payment_id, ack_uid),
                        )
                        .await
                        .unwrap();

                        return None;
                    }
                    PaymentDoneStatus::Success(_, _, _) => unreachable!(),
                }
            }
            _ => continue,
        };
        assert_eq!(payment_commit.payment_id, payment_id);
        break payment_commit.commit;
    };

    // ... Node0 now passes the commit to Node1 out of band ...

    // Node1: Verify the commit:
    let verify_request_id = gen_uid();
    let request_verify_commit = RequestVerifyCommit {
        request_id: verify_request_id.clone(),
        seller_public_key: seller_public_key.clone(),
        commit: commit.clone(),
    };
    send_request(
        &mut conn_pair1,
        UserToCompact::RequestVerifyCommit(request_verify_commit),
    )
    .await
    .unwrap();

    // Node1: Wait for verification of commit:
    loop {
        let compact_to_user_ack = conn_pair1.receiver.next().await.unwrap();
        let response_verify_commit = if let CompactToUserAck::CompactToUser(
            CompactToUser::ResponseVerifyCommit(response_verify_commit),
        ) = compact_to_user_ack
        {
            response_verify_commit
        } else {
            continue;
        };
        assert_eq!(response_verify_commit.request_id, verify_request_id);
        assert_eq!(response_verify_commit.status, VerifyCommitStatus::Success);
        break;
    }

    // Node1: Apply the commit:
    send_request(&mut conn_pair1, UserToCompact::CommitInvoice(commit))
        .await
        .unwrap();

    // Wait some time:
    advance_time(5, &mut tick_sender, &test_executor).await;

    // Node0: Wait for PaymentDone:
    let (opt_receipt_fees, ack_uid) = loop {
        let compact_to_user_ack = conn_pair0.receiver.next().await.unwrap();
        let payment_done =
            if let CompactToUserAck::CompactToUser(CompactToUser::PaymentDone(payment_done)) =
                compact_to_user_ack
            {
                payment_done
            } else {
                continue;
            };
        assert_eq!(payment_done.payment_id, payment_id);
        match payment_done.status {
            PaymentDoneStatus::Success(receipt, fees, ack_uid) => {
                break (Some((receipt, fees)), ack_uid)
            }
            PaymentDoneStatus::Failure(ack_uid) => break (None, ack_uid),
        };
    };

    // Node0: AckPaymentDone:
    send_request(
        &mut conn_pair0,
        UserToCompact::AckPaymentDone(payment_id, ack_uid),
    )
    .await
    .unwrap();

    opt_receipt_fees
}
*/

async fn task_compact_server_two_nodes_payment(mut test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
    let currency3 = Currency::try_from("FST3".to_owned()).unwrap();

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

    let mut compact0 = create_compact_server(
        0,
        sim_db.clone(),
        sim_net_client.clone(),
        timer_client.clone(),
        test_executor.clone(),
    )
    .await
    .unwrap();

    // First incoming message should be nodes status:
    let mut nodes_status0 =
        if let ServerToUserAck::ServerToUser(ServerToUser::NodesStatus(nodes_status)) =
            compact0.receiver.next().await.unwrap()
        {
            nodes_status
        } else {
            unreachable!()
        };

    let mut compact1 = create_compact_server(
        1,
        sim_db.clone(),
        sim_net_client.clone(),
        timer_client.clone(),
        test_executor.clone(),
    )
    .await
    .unwrap();

    // First incoming message should be nodes status:
    let mut nodes_status1 =
        if let ServerToUserAck::ServerToUser(ServerToUser::NodesStatus(nodes_status)) =
            compact1.receiver.next().await.unwrap()
        {
            nodes_status
        } else {
            unreachable!()
        };

    /*
    // Handle reports:
    let (sender0, receiver0) = compact_node0.split();
    let (receiver0, mut compact_report_client0) =
        compact_report_service(compact_report0, receiver0, &test_executor);
    let mut compact_node0 = ConnPair::from_raw(sender0, receiver0);

    let (sender1, receiver1) = compact_node1.split();
    let (receiver1, mut compact_report_client1) =
        compact_report_service(compact_report1, receiver1, &test_executor);
    let mut compact_node1 = ConnPair::from_raw(sender1, receiver1);

    */

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

    let node0_name = NodeName::new("local_node0".to_owned());
    let node1_name = NodeName::new("local_node1".to_owned());

    // compact0: Create a local node:
    let create_node_local = CreateNodeLocal {
        node_name: node0_name.clone(),
    };
    let request_create_node = CreateNode::CreateNodeLocal(create_node_local);
    let user_to_server = UserToServer::CreateNode(request_create_node);
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    // compact1: Create a local node:
    let create_node_local = CreateNodeLocal {
        node_name: node1_name.clone(),
    };
    let request_create_node = CreateNode::CreateNodeLocal(create_node_local);
    let user_to_server = UserToServer::CreateNode(request_create_node);
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;

    // compact0: open a local node:
    let user_to_server = UserToServer::RequestOpenNode(node0_name.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;
    dbg!(&nodes_status0);
    dbg!(compact0.receiver.next().await.unwrap());

    /*
    // compact1: open a local node:
    let user_to_server = UserToServer::RequestOpenNode(node1_name.clone());
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;
    */

    /*

    // Configure relays:
    send_request(
        &mut compact_node0,
        UserToCompact::AddRelay(named_relay_address(0)),
    )
    .await
    .unwrap();

    send_request(
        &mut compact_node1,
        UserToCompact::AddRelay(named_relay_address(1)),
    )
    .await
    .unwrap();

    // Configure index servers:
    send_request(
        &mut compact_node0,
        UserToCompact::AddIndexServer(named_index_server_address(0)),
    )
    .await
    .unwrap();

    send_request(
        &mut compact_node1,
        UserToCompact::AddIndexServer(named_index_server_address(1)),
    )
    .await
    .unwrap();

    // Wait some time:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Add Node1 as a friend:
    let add_friend = AddFriend {
        friend_public_key: node_public_key(1),
        relays: vec![relay_address(1)],
        name: "node1".to_owned(),
    };
    send_request(&mut compact_node0, UserToCompact::AddFriend(add_friend))
        .await
        .unwrap();

    // Node1: Add Node0 as a friend:
    let add_friend = AddFriend {
        friend_public_key: node_public_key(0),
        relays: vec![relay_address(0)],
        name: "node0".to_owned(),
    };
    send_request(&mut compact_node1, UserToCompact::AddFriend(add_friend))
        .await
        .unwrap();

    // Node0: Enable node1:
    send_request(
        &mut compact_node0,
        UserToCompact::EnableFriend(node_public_key(1)),
    )
    .await
    .unwrap();

    // Node1: Enable node1:
    send_request(
        &mut compact_node1,
        UserToCompact::EnableFriend(node_public_key(0)),
    )
    .await
    .unwrap();

    advance_time(10, &mut tick_sender, &test_executor).await;

    // Wait until both sides see each other as online:
    loop {
        let compact_report0 = compact_report_client0.request_report().await;
        let friend_report = match compact_report0.friends.get(&node_public_key(1)) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness == FriendLivenessReport::Online {
            break;
        }
        advance_time(5, &mut tick_sender, &test_executor).await;
    }

    loop {
        let compact_report1 = compact_report_client1.request_report().await;
        let friend_report = match compact_report1.friends.get(&node_public_key(0)) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness == FriendLivenessReport::Online {
            break;
        }
        advance_time(5, &mut tick_sender, &test_executor).await;
    }

    // Node0: Set active currencies for Node1:
    for currency in [&currency1, &currency2, &currency3].into_iter() {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key: node_public_key(1),
            currency: (*currency).clone(),
            rate: Rate::new(),
        };
        send_request(
            &mut compact_node0,
            UserToCompact::SetFriendCurrencyRate(set_friend_currency_rate),
        )
        .await
        .unwrap();
    }

    // Node1: Set active currencies for Node0:
    for currency in [&currency1, &currency2].into_iter() {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key: node_public_key(0),
            currency: (*currency).clone(),
            rate: Rate::new(),
        };
        send_request(
            &mut compact_node1,
            UserToCompact::SetFriendCurrencyRate(set_friend_currency_rate),
        )
        .await
        .unwrap();
    }

    // Wait some time, to let the two nodes negotiate currencies:
    advance_time(10, &mut tick_sender, &test_executor).await;

    for currency in [&currency1, &currency2].into_iter() {
        // Node0: Open currency
        let open_friend_currency = OpenFriendCurrency {
            friend_public_key: node_public_key(1),
            currency: (*currency).clone(),
        };
        send_request(
            &mut compact_node0,
            UserToCompact::OpenFriendCurrency(open_friend_currency),
        )
        .await
        .unwrap();

        // Node1: Open currency
        let open_friend_currency = OpenFriendCurrency {
            friend_public_key: node_public_key(0),
            currency: (*currency).clone(),
        };
        send_request(
            &mut compact_node1,
            UserToCompact::OpenFriendCurrency(open_friend_currency),
        )
        .await
        .unwrap();
    }

    // Wait some time, to let the index servers exchange information:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Node1 allows node0 to have maximum debt of currency1=10
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key: node_public_key(0),
        currency: currency1.clone(),
        remote_max_debt: 10,
    };
    send_request(
        &mut compact_node1,
        UserToCompact::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt),
    )
    .await
    .unwrap();

    // Node1 allows node0 to have maximum debt of currency2=15
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key: node_public_key(0),
        currency: currency2.clone(),
        remote_max_debt: 15,
    };
    send_request(
        &mut compact_node1,
        UserToCompact::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt),
    )
    .await
    .unwrap();

    // Wait until the max debt was set:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Send 10 currency1 credits from node0 to node1:
    let opt_receipt_fees = make_test_payment(
        &mut compact_node0,
        &mut compact_node1,
        node_public_key(0),
        node_public_key(1),
        currency1.clone(),
        10u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_some());

    // Allow some time for the index servers to be updated about the new state:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Send 11 currency2 credits from node0 to node1:
    let opt_receipt_fees = make_test_payment(
        &mut compact_node0,
        &mut compact_node1,
        node_public_key(0),
        node_public_key(1),
        currency2.clone(),
        11u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_some());

    // Allow some time for the index servers to be updated about the new state:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Node1: Send 5 credits to Node0:
    let opt_receipt_fees = make_test_payment(
        &mut compact_node1,
        &mut compact_node0,
        node_public_key(1),
        node_public_key(0),
        currency1.clone(),
        5u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_some());

    // Allow some time for the index servers to be updated about the new state:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Node1: Attempt to send 6 more credits to Node0:
    // This should not work, because 6 + 5 = 11 > 10
    let opt_receipt_fees = make_test_payment(
        &mut compact_node1,
        &mut compact_node0,
        node_public_key(1),
        node_public_key(0),
        currency1.clone(),
        6u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_none());
    */
}

#[test]
fn test_compact_server_two_nodes_payment() {
    let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_compact_server_two_nodes_payment(test_executor.clone()));
    assert!(res.is_output());
}
