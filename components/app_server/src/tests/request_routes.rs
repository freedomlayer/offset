use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn};
use futures::executor::ThreadPool;

use crypto::uid::Uid;
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::uid::UID_LEN;

use proto::app_server::messages::{AppServerToApp, AppToAppServer, AppPermissions, AppRequest};
use proto::index_client::messages::{IndexClientToAppServer, AppServerToIndexClient, IndexClientRequest,
                                    ClientResponseRoutes, ResponseRoutesResult, RequestRoutes};

use super::utils::spawn_dummy_app_server;


async fn task_app_server_loop_request_routes<S>(spawner: S) 
where
    S: Spawn + Clone + Send + 'static,
{

    let (_funder_sender, _funder_receiver,
         mut index_client_sender, mut index_client_receiver,
         mut connections_sender, _initial_node_report) = spawn_dummy_app_server(spawner.clone());

    // Connect two apps:
    let (mut app_sender0, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver0) = mpsc::channel(0);
    let app_server_conn_pair = (app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    };
    await!(connections_sender.send((app_permissions, app_server_conn_pair))).unwrap();

    let (_app_sender1, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver1) = mpsc::channel(0);
    let app_server_conn_pair = (app_server_sender, app_server_receiver);
    let app_permissions = AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    };
    await!(connections_sender.send((app_permissions, app_server_conn_pair))).unwrap();


    // The apps should receive the current node report as the first message:
    let _to_app_message = await!(app_receiver0.next()).unwrap();
    let _to_app_message = await!(app_receiver1.next()).unwrap();

    // Send a request routes message through app0:
    let request_routes = RequestRoutes {
        request_id: Uid::from(&[3; UID_LEN]),
        capacity: 250,
        source: PublicKey::from(&[0xee; PUBLIC_KEY_LEN]),
        destination: PublicKey::from(&[0xff; PUBLIC_KEY_LEN]),
        opt_exclude: None,
    };

    let to_app_server = AppToAppServer::new(Uid::from(&[22; UID_LEN]),
        AppRequest::RequestRoutes(request_routes.clone()));
    await!(app_sender0.send(to_app_server)).unwrap();

    // RequestRoutes command should be forwarded to IndexClient:
    let to_index_client_message = await!(index_client_receiver.next()).unwrap();
    match to_index_client_message {
        AppServerToIndexClient::AppRequest((app_request_id, IndexClientRequest::RequestRoutes(received_request_routes))) => {
            assert_eq!(app_request_id, Uid::from(&[22; UID_LEN]));
            assert_eq!(received_request_routes, request_routes);
        },
        _ => unreachable!(),
    };

    // IndexClient returns a response that is not related to any open request.
    // This response will be discarded.
    let client_response_routes = ClientResponseRoutes {
        request_id: Uid::from(&[2; UID_LEN]),
        result: ResponseRoutesResult::Failure,
    };
    await!(index_client_sender.send(IndexClientToAppServer::ResponseRoutes(client_response_routes))).unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());

    // IndexClient returns a response corresponding to an open request:
    let client_response_routes = ClientResponseRoutes {
        request_id: Uid::from(&[3; UID_LEN]),
        result: ResponseRoutesResult::Failure,
    };
    await!(index_client_sender.send(IndexClientToAppServer::ResponseRoutes(client_response_routes))).unwrap();

    let to_app_message = await!(app_receiver0.next()).unwrap();
    match to_app_message {
        AppServerToApp::ResponseRoutes(response_routes) => {
            assert_eq!(response_routes.request_id, Uid::from(&[3; UID_LEN]));
            assert_eq!(response_routes.result, ResponseRoutesResult::Failure);
        },
        _ => unreachable!(),
    }
    // We shouldn't get an incoming message at app1:
    assert!(app_receiver1.try_next().is_err());

    // IndexClient again returns the same response.
    // This time the response should be discarded, 
    // because it does not correspond to any open request.
    let client_response_routes = ClientResponseRoutes {
        request_id: Uid::from(&[3; UID_LEN]),
        result: ResponseRoutesResult::Failure,
    };
    await!(index_client_sender.send(IndexClientToAppServer::ResponseRoutes(client_response_routes))).unwrap();

    // We shouldn't get an message at any of the apps:
    assert!(app_receiver0.try_next().is_err());
    assert!(app_receiver1.try_next().is_err());
}

#[test]
fn test_app_server_loop_index_request_routes() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_app_server_loop_request_routes(thread_pool.clone()));
}
