use futures::channel::{mpsc, oneshot};
use futures::executor::{block_on, ThreadPool};
use futures::task::Spawn;
use futures::{SinkExt, StreamExt};

use common::conn::ConnPair;

use proto::app_server::messages::{
    AppPermissions, AppRequest, AppServerToApp, AppToAppServer, NodeReportMutation,
};
use proto::crypto::{PublicKey, Uid};
use proto::index_client::messages::{
    AppServerToIndexClient, IndexClientReportMutation, IndexClientReportMutations,
    IndexClientRequest, IndexClientToAppServer,
};
use proto::index_server::messages::NamedIndexServerAddress;

use super::utils::spawn_dummy_app_server;
use crate::server::IncomingAppConnection;

async fn task_app_server_loop_index_client_command<S>(spawner: S)
where
    S: Spawn + Clone + Send + 'static,
{
    let (
        _funder_sender,
        _funder_receiver,
        mut index_client_sender,
        mut index_client_receiver,
        mut connections_sender,
        initial_node_report,
    ) = spawn_dummy_app_server(spawner.clone());

    let (mut app_sender, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver) = mpsc::channel(0);
    let server_conn_pair = ConnPair::from_raw(app_server_sender, app_server_receiver);

    let app_permissions = AppPermissions {
        routes: true,
        buyer: true,
        seller: true,
        config: true,
    };

    let (report_sender, report_receiver) = oneshot::channel();
    let incoming_app_connection = IncomingAppConnection {
        app_permissions,
        report_sender,
    };

    connections_sender
        .send(incoming_app_connection)
        .await
        .unwrap();

    let (report, conn_sender) = report_receiver.await.unwrap();
    conn_sender.send(server_conn_pair).unwrap();

    // Verify the report:
    assert_eq!(report, initial_node_report);

    // Send a command through the app:
    let named_index_server_address = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
        address: 300u32,
        name: "IndexServer300".to_string(),
    };
    let to_app_server = AppToAppServer {
        app_request_id: Uid::from(&[11; Uid::len()]),
        app_request: AppRequest::AddIndexServer(named_index_server_address.clone()),
    };
    app_sender.send(to_app_server).await.unwrap();

    // AddIndexServer command should be forwarded to IndexClient, in the form of AddIndexServer:
    let to_index_client_message = index_client_receiver.next().await.unwrap();
    match to_index_client_message {
        AppServerToIndexClient::AppRequest((
            _app_request_id,
            IndexClientRequest::AddIndexServer(named_index_server_address0),
        )) => assert_eq!(named_index_server_address0, named_index_server_address),
        _ => unreachable!(),
    };

    let named_index_server_address = NamedIndexServerAddress {
        public_key: PublicKey::from(&[0xaa; PublicKey::len()]),
        address: 300u32,
        name: "IndexServer300".to_string(),
    };
    let index_client_report_mutation =
        IndexClientReportMutation::AddIndexServer(named_index_server_address);
    let mutations = vec![index_client_report_mutation.clone()];
    let index_client_report_mutations = IndexClientReportMutations {
        opt_app_request_id: Some(Uid::from(&[11; Uid::len()])),
        mutations,
    };
    index_client_sender
        .send(IndexClientToAppServer::ReportMutations(
            index_client_report_mutations,
        ))
        .await
        .unwrap();

    let to_app_message = app_receiver.next().await.unwrap();
    match to_app_message {
        AppServerToApp::ReportMutations(report_mutations) => {
            assert_eq!(report_mutations.mutations.len(), 1);
            let report_mutation = &report_mutations.mutations[0];
            match report_mutation {
                NodeReportMutation::IndexClient(received_index_client_report_mutation) => {
                    assert_eq!(
                        received_index_client_report_mutation,
                        &index_client_report_mutation
                    );
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_app_server_loop_index_client_command() {
    let thread_pool = ThreadPool::new().unwrap();
    block_on(task_app_server_loop_index_client_command(
        thread_pool.clone(),
    ));
}
