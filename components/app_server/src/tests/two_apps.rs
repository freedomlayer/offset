use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::executor::ThreadPool;
use futures::{FutureExt, TryFutureExt};

use im::hashmap::HashMap as ImHashMap;

use crypto::uid::Uid;
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::uid::UID_LEN;

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl,
                                UserRequestSendFunds, FriendsRoute, 
                                InvoiceId, INVOICE_ID_LEN, ResponseReceived, 
                                ResponseSendFundsResult};
use proto::funder::report::FunderReport;
use proto::app_server::messages::{AppServerToApp, AppToAppServer, NodeReport,
                                    NodeReportMutation};
use proto::index_client::messages::{IndexClientToAppServer, AppServerToIndexClient};
use proto::index_client::messages::{IndexClientReport, IndexClientReportMutation, 
    ClientResponseRoutes, ResponseRoutesResult, RequestRoutes};

use crate::config::AppPermissions;
use crate::server::{IncomingAppConnection, app_server_loop};

use super::utils::spawn_dummy_app_server;


async fn task_app_server_loop_two_apps<S>(mut spawner: S) 
where
    S: Spawn + Clone + Send + 'static,
{

    let (mut funder_sender, mut funder_receiver,
         mut index_client_sender, mut index_client_receiver,
         mut connections_sender, initial_node_report) = spawn_dummy_app_server(spawner.clone());

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

    let (mut app_sender1, app_server_receiver) = mpsc::channel(0);
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
    // Send a report 
    let to_app_message = await!(app_receiver0.next()).unwrap();
    match to_app_message {
        AppServerToApp::Report(report) => assert_eq!(report, initial_node_report),
        _ => unreachable!(),
    };
    let to_app_message = await!(app_receiver1.next()).unwrap();
    match to_app_message {
        AppServerToApp::Report(report) => assert_eq!(report, initial_node_report),
        _ => unreachable!(),
    };

    // Send a dummy report message from IndexClient:
    let new_index_server_address = 300u64;
    let index_client_report_mutation = IndexClientReportMutation::AddIndexServer(new_index_server_address.clone());
    let index_client_report_mutations = vec![index_client_report_mutation.clone()];
    await!(index_client_sender.send(IndexClientToAppServer::ReportMutations(index_client_report_mutations))).unwrap();

    // Both apps should get the report:
    let to_app_message = await!(app_receiver0.next()).unwrap(); 
    match to_app_message {
        AppServerToApp::ReportMutations(report_mutations) => {
            assert_eq!(report_mutations.len(), 1);
            let report_mutation = &report_mutations[0];
            match report_mutation {
                NodeReportMutation::IndexClient(received_index_client_report_mutation) => {
                    assert_eq!(received_index_client_report_mutation, 
                               &index_client_report_mutation);
                },
                _ => unreachable!(),
            }
        },
        _ => unreachable!(),
    }
    let to_app_message = await!(app_receiver1.next()).unwrap(); 
    match to_app_message {
        AppServerToApp::ReportMutations(report_mutations) => {
            assert_eq!(report_mutations.len(), 1);
            let report_mutation = &report_mutations[0];
            match report_mutation {
                NodeReportMutation::IndexClient(received_index_client_report_mutation) => {
                    assert_eq!(received_index_client_report_mutation, 
                               &index_client_report_mutation);
                },
                _ => unreachable!(),
            }
        },
        _ => unreachable!(),
    }
}

#[test]
fn test_app_server_loop_index_two_apps() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_app_server_loop_two_apps(thread_pool.clone()));
}
