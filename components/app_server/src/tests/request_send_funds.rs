use std::convert::TryFrom;

use futures::channel::mpsc;
use futures::executor::{ThreadPool, block_on};
use futures::task::Spawn;
use futures::{SinkExt, StreamExt};

use common::conn::ConnPair;

use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};

use proto::app_server::messages::{AppPermissions, AppRequest, AppServerToApp, AppToAppServer};
use proto::funder::messages::{
    CreatePayment, CreateTransaction, Currency, FriendsRoute, FunderControl, FunderOutgoingControl,
    RequestResult, TransactionResult,
};

use super::utils::spawn_dummy_app_server;

async fn task_app_server_loop_request_send_funds<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let (
        mut funder_sender,
        mut funder_receiver,
        _index_client_sender,
        _index_client_receiver,
        mut connections_sender,
        _initial_node_report,
    ) = spawn_dummy_app_server(spawner.clone());

    // Connect two apps:
    let (mut app_sender0, app_server_receiver) = mpsc::channel(1);
    let (app_server_sender, mut app_receiver0) = mpsc::channel(1);
    let app_server_conn_pair = ConnPair::from_raw(app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        routes: true,
        buyer: true,
        seller: true,
        config: true,
    };
    connections_sender
        .send((app_permissions, app_server_conn_pair))
        .await
        .unwrap();

    let (_app_sender1, app_server_receiver) = mpsc::channel(1);
    let (app_server_sender, mut app_receiver1) = mpsc::channel(1);
    let app_server_conn_pair = ConnPair::from_raw(app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        routes: true,
        buyer: true,
        seller: true,
        config: true,
    };
    connections_sender
        .send((app_permissions, app_server_conn_pair))
        .await
        .unwrap();

    // The apps should receive the current node report as the first message:
    let _to_app_message = app_receiver0.next().await.unwrap();
    let _to_app_message = app_receiver1.next().await.unwrap();

    let pk_e = PublicKey::from(&[0xee; PublicKey::len()]);
    let pk_f = PublicKey::from(&[0xff; PublicKey::len()]);

    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();

    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[1; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[2; InvoiceId::len()]),
        currency: currency1.clone(),
        total_dest_payment: 20,
        dest_public_key: pk_f.clone(),
    };
    let to_app_server = AppToAppServer::new(
        Uid::from(&[22; Uid::len()]),
        AppRequest::CreatePayment(create_payment.clone()),
    );
    app_sender0.send(to_app_server).await.unwrap();

    // CreatePayment command should be forwarded to the Funder:
    let funder_incoming_control = funder_receiver.next().await.unwrap();
    assert_eq!(
        funder_incoming_control.app_request_id,
        Uid::from(&[22; Uid::len()])
    );
    match funder_incoming_control.funder_control {
        FunderControl::CreatePayment(received_create_payment) => {
            assert_eq!(received_create_payment, create_payment)
        }
        _ => unreachable!(),
    };

    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[1; PaymentId::len()]),
        request_id: Uid::from(&[3; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![pk_e.clone(), pk_f.clone()],
        },
        dest_payment: 20,
        fees: 4,
    };
    let to_app_server = AppToAppServer::new(
        Uid::from(&[23; Uid::len()]),
        AppRequest::CreateTransaction(create_transaction.clone()),
    );
    app_sender0.send(to_app_server).await.unwrap();

    // CreateTransaction command should be forwarded to the Funder:
    let funder_incoming_control = funder_receiver.next().await.unwrap();
    assert_eq!(
        funder_incoming_control.app_request_id,
        Uid::from(&[23; Uid::len()])
    );
    match funder_incoming_control.funder_control {
        FunderControl::CreateTransaction(received_create_transaction) => {
            assert_eq!(received_create_transaction, create_transaction)
        }
        _ => unreachable!(),
    };

    // Funder returns a TransactionResult that is not related to any open request.
    let transaction_result = TransactionResult {
        request_id: Uid::from(&[2; Uid::len()]),
        result: RequestResult::Failure,
    };
    funder_sender
        .send(FunderOutgoingControl::TransactionResult(transaction_result))
        .await
        .unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());

    // Funder returns a response that corresponds to the open request:
    let transaction_result = TransactionResult {
        request_id: Uid::from(&[3; Uid::len()]),
        result: RequestResult::Failure,
    };
    funder_sender
        .send(FunderOutgoingControl::TransactionResult(
            transaction_result.clone(),
        ))
        .await
        .unwrap();

    let to_app_message = app_receiver0.next().await.unwrap();
    match to_app_message {
        AppServerToApp::TransactionResult(received_transaction_result) => {
            assert_eq!(received_transaction_result, transaction_result);
        }
        _ => unreachable!(),
    }
    // We shouldn't get an incoming message at app1:
    assert!(app_receiver1.try_next().is_err());

    // Funder again returns the same response,
    // however, this time it will be discarded, because no open request
    // has a matching id:
    let transaction_result = TransactionResult {
        request_id: Uid::from(&[3; Uid::len()]),
        result: RequestResult::Failure,
    };
    funder_sender
        .send(FunderOutgoingControl::TransactionResult(transaction_result))
        .await
        .unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());
}

#[test]
fn test_app_server_loop_index_request_send_funds() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_app_server_loop_request_send_funds(thread_pool.clone()));
}
