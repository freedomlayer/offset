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
    CreateNode, CreateNodeLocal, CreateNodeRemote, NodeId, NodeInfo, NodeInfoLocal, NodeName,
    NodesStatus, ResponseOpenNode, ServerToUser, ServerToUserAck, UserToServer, UserToServerAck,
};

use crate::compact_server_wrapper::send_request;
use crate::sim_network::create_sim_network;
use crate::utils::{
    advance_time, app_private_key, create_compact_node, create_compact_server, create_index_server,
    create_node, create_relay, listen_node_address, named_index_server_address,
    named_relay_address, node_public_key, relay_address, SimDb,
};

const TIMER_CHANNEL_LEN: usize = 0;

/// Helper function: Send a UserToCompact message to a specific node
async fn node_request(
    compact: &mut ConnPair<UserToServerAck, ServerToUserAck>,
    nodes_status: &mut NodesStatus,
    node_id: NodeId,
    user_to_compact: UserToCompact,
) {
    let user_to_server = UserToServer::Node(node_id.clone(), user_to_compact);
    send_request(compact, nodes_status, user_to_server)
        .await
        .unwrap();
}

use crate::compact_report_service::compact_report_service;

/// Perform a basic payment between a buyer and a seller.
/// Node0 sends credits to Node1
async fn make_test_payment(
    mut compact0: &mut ConnPair<UserToServerAck, ServerToUserAck>,
    mut nodes_status0: &mut NodesStatus,
    node_id0: NodeId,
    mut compact1: &mut ConnPair<UserToServerAck, ServerToUserAck>,
    mut nodes_status1: &mut NodesStatus,
    node_id1: NodeId,
    _buyer_public_key: PublicKey,
    seller_public_key: PublicKey,
    currency: Currency,
    total_dest_payment: u128,
    mut tick_sender: mpsc::Sender<()>,
    test_executor: TestExecutor,
) -> Option<(Receipt, u128)> {
    let payment_id = PaymentId::from(&[4u8; PaymentId::len()]);
    let invoice_id = InvoiceId::from(&[3u8; InvoiceId::len()]);

    // Node1: Add invoice:
    let add_invoice = AddInvoice {
        invoice_id: invoice_id.clone(),
        currency: currency.clone(),
        total_dest_payment,
        description: "Example payment".to_owned(),
    };
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::AddInvoice(add_invoice),
    )
    .await;

    // Node0: Init payment
    let init_payment = InitPayment {
        payment_id: payment_id.clone(),
        invoice_id: invoice_id.clone(),
        currency: currency.clone(),
        dest_public_key: seller_public_key.clone(),
        dest_payment: total_dest_payment,
        description: "Example payment".to_owned(),
    };
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::InitPayment(init_payment),
    )
    .await;

    // Node0: Wait for payment fees:
    let (_fees, confirm_id) = loop {
        let server_to_user_ack = compact0.receiver.next().await.unwrap();
        let payment_fees = if let ServerToUserAck::ServerToUser(ServerToUser::Node(
            node_id,
            CompactToUser::PaymentFees(payment_fees),
        )) = server_to_user_ack
        {
            assert_eq!(node_id, node_id0);
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
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::ConfirmPaymentFees(confirm_payment_fees),
    )
    .await;

    // Node0: Wait for commit to be created:
    let commit = loop {
        let server_to_user_ack = compact0.receiver.next().await.unwrap();
        let payment_commit = match server_to_user_ack {
            ServerToUserAck::ServerToUser(ServerToUser::Node(
                node_id,
                CompactToUser::PaymentCommit(payment_commit),
            )) => payment_commit,
            ServerToUserAck::ServerToUser(ServerToUser::Node(
                node_id,
                CompactToUser::PaymentDone(payment_done),
            )) => {
                // We get here if the remote side rejects our request for payment:
                assert_eq!(payment_done.payment_id, payment_id);
                assert_eq!(node_id, node_id0);
                match payment_done.status {
                    PaymentDoneStatus::Failure(ack_uid) => {
                        // Node0: AckPaymentDone:
                        node_request(
                            &mut compact0,
                            &mut nodes_status0,
                            node_id0.clone(),
                            UserToCompact::AckPaymentDone(payment_id, ack_uid),
                        )
                        .await;

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
        commit: commit.clone(),
    };
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::RequestVerifyCommit(request_verify_commit),
    )
    .await;

    // Node1: Wait for verification of commit:
    loop {
        let compact_to_user_ack = compact1.receiver.next().await.unwrap();
        let response_verify_commit = if let ServerToUserAck::ServerToUser(ServerToUser::Node(
            node_id,
            CompactToUser::ResponseVerifyCommit(response_verify_commit),
        )) = compact_to_user_ack
        {
            assert_eq!(node_id, node_id1);
            response_verify_commit
        } else {
            continue;
        };
        assert_eq!(response_verify_commit.request_id, verify_request_id);
        assert_eq!(response_verify_commit.status, VerifyCommitStatus::Success);
        break;
    }

    // Node1: Apply the commit:
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::CommitInvoice(commit.invoice_id.clone()),
    )
    .await;

    // Wait some time:
    advance_time(5, &mut tick_sender, &test_executor).await;

    // Node0: Wait for PaymentDone:
    let (opt_receipt_fees, ack_uid) = loop {
        let compact_to_user_ack = compact0.receiver.next().await.unwrap();
        let payment_done = if let ServerToUserAck::ServerToUser(ServerToUser::Node(
            node_id,
            CompactToUser::PaymentDone(payment_done),
        )) = compact_to_user_ack
        {
            assert_eq!(node_id, node_id0);
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
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::AckPaymentDone(payment_id, ack_uid),
    )
    .await;

    opt_receipt_fees
}

async fn task_compact_server_remote_node(mut test_executor: TestExecutor) {
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

    // Create a node that will be used as a remote node for compact1:
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

    let pk0 =
        if let NodeInfo::Local(node_info_local) = &nodes_status0.get(&node0_name).unwrap().info {
            node_info_local.node_public_key.clone()
        } else {
            unreachable!();
        };

    // compact1: Create a remote node:
    let create_node_remote = CreateNodeRemote {
        node_name: node1_name.clone(),
        app_private_key: app_private_key(1),
        node_public_key: node_public_key(1),
        node_address: listen_node_address(1),
    };
    let request_create_node = CreateNode::CreateNodeRemote(create_node_remote);
    let user_to_server = UserToServer::CreateNode(request_create_node);
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;

    let pk1 =
        if let NodeInfo::Remote(node_info_remote) = &nodes_status1.get(&node1_name).unwrap().info {
            node_info_remote.node_public_key.clone()
        } else {
            unreachable!();
        };

    // compact0: open a local node:
    let user_to_server = UserToServer::RequestOpenNode(node0_name.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    // Wait for response:
    let server_to_user_ack = compact0.receiver.next().await.unwrap();
    let node_id0 = if let ServerToUserAck::ServerToUser(ServerToUser::ResponseOpenNode(
        ResponseOpenNode::Success(_node_name, node_id, _app_permissions, _compact_report),
    )) = server_to_user_ack
    {
        node_id
    } else {
        unreachable!();
    };

    // compact1: open a local node:
    let user_to_server = UserToServer::RequestOpenNode(node1_name.clone());
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;

    // Wait for response:
    let server_to_user_ack = compact1.receiver.next().await.unwrap();
    let node_id1 = if let ServerToUserAck::ServerToUser(ServerToUser::ResponseOpenNode(
        ResponseOpenNode::Success(_node_name, node_id, _app_permissions, _compact_report),
    )) = server_to_user_ack
    {
        node_id
    } else {
        unreachable!();
    };

    // Configure relays:
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::AddRelay(named_relay_address(0)),
    )
    .await;

    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::AddRelay(named_relay_address(1)),
    )
    .await;

    // Configure index servers:
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::AddIndexServer(named_index_server_address(0)),
    )
    .await;

    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::AddIndexServer(named_index_server_address(1)),
    )
    .await;

    // Wait some time:
    advance_time(40, &mut tick_sender, &test_executor).await;

    // Node0: Add Node1 as a friend:
    let add_friend = AddFriend {
        friend_public_key: pk1.clone(),
        relays: vec![relay_address(1)],
        name: "node1".to_owned(),
    };
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::AddFriend(add_friend),
    )
    .await;

    // Node1: Add Node0 as a friend:
    let add_friend = AddFriend {
        friend_public_key: pk0.clone(),
        relays: vec![relay_address(0)],
        name: "node0".to_owned(),
    };
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::AddFriend(add_friend),
    )
    .await;

    // Node0: Enable node1:
    node_request(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        UserToCompact::EnableFriend(pk1.clone()),
    )
    .await;

    // Node1: Enable node0:
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::EnableFriend(pk0.clone()),
    )
    .await;

    advance_time(10, &mut tick_sender, &test_executor).await;

    // TODO: Possibly add waiting for both sides to be online in the future.
    // This requires some mechanism for reading compact reports while passing time.

    /*

    // Wait until both sides see each other as online:
    loop {
        let compact_report0 = compact_report_client0.request_report().await;
        let friend_report = match compact_report0.friends.get(&pk1)) {
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
        let friend_report = match compact_report1.friends.get(&pk0) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        if friend_report.liveness == FriendLivenessReport::Online {
            break;
        }
        advance_time(5, &mut tick_sender, &test_executor).await;
    }

    */

    // Node0: Set active currencies for Node1:
    for currency in [&currency1, &currency2, &currency3].into_iter() {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key: pk1.clone(),
            currency: (*currency).clone(),
            rate: Rate::new(),
        };
        // Node0: Enable node1:
        node_request(
            &mut compact0,
            &mut nodes_status0,
            node_id0.clone(),
            UserToCompact::SetFriendCurrencyRate(set_friend_currency_rate),
        )
        .await;
    }

    // Node1: Set active currencies for Node0:
    for currency in [&currency1, &currency2].into_iter() {
        let set_friend_currency_rate = SetFriendCurrencyRate {
            friend_public_key: pk0.clone(),
            currency: (*currency).clone(),
            rate: Rate::new(),
        };
        node_request(
            &mut compact1,
            &mut nodes_status1,
            node_id1.clone(),
            UserToCompact::SetFriendCurrencyRate(set_friend_currency_rate),
        )
        .await;
    }

    // Wait some time, to let the two nodes negotiate currencies:
    advance_time(10, &mut tick_sender, &test_executor).await;

    for currency in [&currency1, &currency2].into_iter() {
        // Node0: Open currency
        let open_friend_currency = OpenFriendCurrency {
            friend_public_key: pk1.clone(),
            currency: (*currency).clone(),
        };
        node_request(
            &mut compact0,
            &mut nodes_status0,
            node_id0.clone(),
            UserToCompact::OpenFriendCurrency(open_friend_currency),
        )
        .await;

        // Node1: Open currency
        let open_friend_currency = OpenFriendCurrency {
            friend_public_key: pk0.clone(),
            currency: (*currency).clone(),
        };
        node_request(
            &mut compact1,
            &mut nodes_status1,
            node_id1.clone(),
            UserToCompact::OpenFriendCurrency(open_friend_currency),
        )
        .await;
    }

    // Wait some time, to let the index servers exchange information:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Node1 allows node0 to have maximum debt of currency1=10
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key: pk0.clone(),
        currency: currency1.clone(),
        remote_max_debt: 10,
    };
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt),
    )
    .await;

    // Node1 allows node0 to have maximum debt of currency2=15
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key: pk0.clone(),
        currency: currency2.clone(),
        remote_max_debt: 15,
    };
    node_request(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        UserToCompact::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt),
    )
    .await;

    // Wait until the max debt was set:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // Send 10 currency1 credits from node0 to node1:
    let opt_receipt_fees = make_test_payment(
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        pk0.clone(),
        pk1.clone(),
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
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        pk0.clone(),
        pk1.clone(),
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
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        pk1.clone(),
        pk0.clone(),
        currency1.clone(),
        5u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_some());

    // Allow some time for the index servers to be updated about the new state:
    advance_time(10, &mut tick_sender, &test_executor).await;

    // compact0: close a local node:
    let user_to_server = UserToServer::CloseNode(node_id0.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    advance_time(10, &mut tick_sender, &test_executor).await;

    // Close and open node0:
    // compact0: close a local node:
    let user_to_server = UserToServer::CloseNode(node_id0.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    // compact0: open a local node:
    let user_to_server = UserToServer::RequestOpenNode(node0_name.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    // Wait for response:
    let server_to_user_ack = compact0.receiver.next().await.unwrap();
    let node_id0 = if let ServerToUserAck::ServerToUser(ServerToUser::ResponseOpenNode(
        ResponseOpenNode::Success(_node_name, node_id, _app_permissions, _compact_report),
    )) = server_to_user_ack
    {
        node_id
    } else {
        unreachable!();
    };

    advance_time(10, &mut tick_sender, &test_executor).await;

    // Node1: Attempt to send 6 more credits to Node0:
    // This should not work, because 6 + 5 = 11 > 10
    let opt_receipt_fees = make_test_payment(
        &mut compact1,
        &mut nodes_status1,
        node_id1.clone(),
        &mut compact0,
        &mut nodes_status0,
        node_id0.clone(),
        pk1.clone(),
        pk0.clone(),
        currency1.clone(),
        6u128, // total_dest_payment
        tick_sender.clone(),
        test_executor.clone(),
    )
    .await;

    assert!(opt_receipt_fees.is_none());

    // compact0: close a node:
    let user_to_server = UserToServer::CloseNode(node_id0.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;

    // Remove node0:
    let user_to_server = UserToServer::RemoveNode(node0_name.clone());
    send_request(&mut compact0, &mut nodes_status0, user_to_server).await;
    assert!(nodes_status0.get(&node0_name).is_none());

    // compact1: close a node:
    let user_to_server = UserToServer::CloseNode(node_id1.clone());
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;

    // Remove node1:
    let user_to_server = UserToServer::RemoveNode(node1_name.clone());
    send_request(&mut compact1, &mut nodes_status1, user_to_server).await;
    assert!(nodes_status1.get(&node1_name).is_none());
}

#[test]
fn test_compact_server_remote_node() {
    // let _ = env_logger::init();
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_compact_server_remote_node(test_executor.clone()));
    assert!(res.is_output());
}
