use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::Spawn;
use futures::executor::ThreadPool;


use crypto::uid::Uid;
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::uid::UID_LEN;

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl,
                                UserRequestSendFunds, FriendsRoute, 
                                InvoiceId, INVOICE_ID_LEN, ResponseReceived, 
                                ResponseSendFundsResult};
use proto::app_server::messages::{AppServerToApp, AppToAppServer, AppPermissions};

use super::utils::spawn_dummy_app_server;


async fn task_app_server_loop_request_send_funds<S>(spawner: S) 
where
    S: Spawn + Clone + Send + 'static,
{

    let (mut funder_sender, mut funder_receiver,
         _index_client_sender, _index_client_receiver,
         mut connections_sender, _initial_node_report) = spawn_dummy_app_server(spawner.clone());

    // Connect two apps:
    let (mut app_sender0, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver0) = mpsc::channel(0);
    let app_server_conn_pair = (app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        reports: true,
        routes: true,
        send_funds: true,
        config: true,
    };
    await!(connections_sender.send((app_permissions, app_server_conn_pair))).unwrap();

    let (_app_sender1, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver1) = mpsc::channel(0);
    let app_server_conn_pair = (app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        reports: true,
        routes: true,
        send_funds: true,
        config: true,
    };
    await!(connections_sender.send((app_permissions, app_server_conn_pair))).unwrap();


    // The apps should receive the current node report as the first message:
    let _to_app_message = await!(app_receiver0.next()).unwrap();
    let _to_app_message = await!(app_receiver1.next()).unwrap();

    let pk_e = PublicKey::from(&[0xee; PUBLIC_KEY_LEN]);
    let pk_f = PublicKey::from(&[0xff; PUBLIC_KEY_LEN]);

    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![pk_e.clone(), pk_f.clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 20,
    };

    await!(app_sender0.send(AppToAppServer::RequestSendFunds(user_request_send_funds.clone()))).unwrap();

    // RequestRoutes command should be forwarded to IndexClient:
    let to_funder_message = await!(funder_receiver.next()).unwrap();
    match to_funder_message {
        FunderIncomingControl::RequestSendFunds(received_user_request_send_funds) => 
            assert_eq!(received_user_request_send_funds, user_request_send_funds),
        _ => unreachable!(),
    };

    // Funder returns a response that is not related to any open request.
    let response_received = ResponseReceived {
        request_id: Uid::from(&[2; UID_LEN]),
        result: ResponseSendFundsResult::Failure(pk_e.clone()),
    };
    await!(funder_sender.send(FunderOutgoingControl::ResponseReceived(response_received))).unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());

    // Funder returns a response that corresponds to the open request:
    let response_received = ResponseReceived {
        request_id: Uid::from(&[3; UID_LEN]),
        result: ResponseSendFundsResult::Failure(pk_e.clone()),
    };
    await!(funder_sender.send(FunderOutgoingControl::ResponseReceived(response_received.clone()))).unwrap();

    let to_app_message = await!(app_receiver0.next()).unwrap();
    match to_app_message {
        AppServerToApp::ResponseReceived(obtained_response_received) => {
            assert_eq!(obtained_response_received, response_received);
        },
        _ => unreachable!(),
    }
    // We shouldn't get an incoming message at app1:
    assert!(app_receiver1.try_next().is_err());

    // Funder again returns the same response,
    // however, this time it will be discarded, because no open request
    // has a matching id:
    let response_received = ResponseReceived {
        request_id: Uid::from(&[3; UID_LEN]),
        result: ResponseSendFundsResult::Failure(pk_e),
    };
    await!(funder_sender.send(FunderOutgoingControl::ResponseReceived(response_received))).unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());
}

#[test]
fn test_app_server_loop_index_request_send_funds() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_app_server_loop_request_send_funds(thread_pool.clone()));
}
