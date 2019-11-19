use futures::channel::mpsc;
use futures::executor::{block_on, ThreadPool};
use futures::task::Spawn;
use futures::{SinkExt, StreamExt};

use common::conn::ConnPair;

use proto::crypto::PublicKey;

use proto::app_server::messages::{AppPermissions, AppServerToApp};
use proto::index_client::messages::{
    IndexClientReportMutation, IndexClientReportMutations, IndexClientToAppServer,
};
use proto::index_server::messages::NamedIndexServerAddress;

use super::utils::spawn_dummy_app_server;

async fn task_app_server_loop_all_apps_closed<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let (
        _funder_sender,
        mut funder_receiver,
        mut index_client_sender,
        mut index_client_receiver,
        mut connections_sender,
        initial_node_report,
    ) = spawn_dummy_app_server(spawner.clone());

    let (app_sender, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver) = mpsc::channel(0);
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

    // The app should receive the current node report as the first message:
    let to_app_message = app_receiver.next().await.unwrap();
    match to_app_message {
        AppServerToApp::Report(report) => assert_eq!(report, initial_node_report),
        _ => unreachable!(),
    };

    drop(connections_sender);

    // Communication with the last connected app should still work:
    let named_index_server_address = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
        address: 300u32,
        name: "IndexServer300".to_string(),
    };
    let index_client_report_mutation =
        IndexClientReportMutation::AddIndexServer(named_index_server_address);
    let mutations = vec![index_client_report_mutation.clone()];
    let index_client_report_mutations = IndexClientReportMutations {
        opt_app_request_id: None,
        mutations,
    };
    index_client_sender
        .send(IndexClientToAppServer::ReportMutations(
            index_client_report_mutations,
        ))
        .await
        .unwrap();

    let _to_app_message = app_receiver.next().await.unwrap();

    // Last app disconnects:
    drop(app_sender);

    // At this point the app_server loop should close.

    assert!(funder_receiver.next().await.is_none());
    assert!(index_client_receiver.next().await.is_none());
}

#[test]
fn test_app_server_loop_index_all_apps_closed() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_app_server_loop_all_apps_closed(thread_pool.clone()));
}
