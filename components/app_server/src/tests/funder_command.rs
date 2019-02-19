use futures::{StreamExt, SinkExt};
use futures::channel::mpsc;
use futures::task::Spawn;
use futures::executor::ThreadPool;

use crypto::uid::{Uid, UID_LEN};

use proto::funder::messages::{FunderOutgoingControl, FunderControl};
use proto::app_server::messages::{AppServerToApp, AppToAppServer, NodeReportMutation,
                                    AppPermissions, AppRequest};
use proto::report::messages::{FunderReportMutation, FunderReportMutations};

use super::utils::{spawn_dummy_app_server, dummy_named_relay_address};

async fn task_app_server_loop_funder_command<S>(spawner: S) 
where
    S: Spawn + Clone + Send + 'static,
{

    let (mut funder_sender, mut funder_receiver,
         _index_client_sender, _index_client_receiver,
         mut connections_sender, initial_node_report) = spawn_dummy_app_server(spawner.clone());

    let (mut app_sender, app_server_receiver) = mpsc::channel(0);
    let (app_server_sender, mut app_receiver) = mpsc::channel(0);
    let app_server_conn_pair = (app_server_sender, app_server_receiver);

    let app_permissions = AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    };

    await!(connections_sender.send((app_permissions, app_server_conn_pair))).unwrap();

    // The app should receive the current node report as the first message:
    let to_app_message = await!(app_receiver.next()).unwrap();
    match to_app_message {
        AppServerToApp::Report(report) => assert_eq!(report, initial_node_report),
        _ => unreachable!(),
    };

    // Send a command through the app:
    let funder_command = AppToAppServer::new(Uid::from(&[22; UID_LEN]),
                AppRequest::AddRelay(dummy_named_relay_address(0)));
    await!(app_sender.send(funder_command)).unwrap();

    // SetRelays command should be forwarded to the Funder:
    let to_funder_message = await!(funder_receiver.next()).unwrap();
    assert_eq!(to_funder_message.app_request_id, Uid::from(&[22; UID_LEN]));
    match to_funder_message.funder_control {
        FunderControl::AddRelay(address) => assert_eq!(address, dummy_named_relay_address(0)),
        _ => unreachable!(),
    };

    let funder_report_mutation = FunderReportMutation::AddRelay(dummy_named_relay_address(0));
    let mutations = vec![funder_report_mutation.clone()];
    let funder_report_mutations = FunderReportMutations {
        opt_app_request_id: Some(Uid::from(&[22; UID_LEN])),
        mutations,
    };
    await!(funder_sender.send(FunderOutgoingControl::ReportMutations(funder_report_mutations))).unwrap();

    let to_app_message = await!(app_receiver.next()).unwrap();
    match to_app_message {
        AppServerToApp::ReportMutations(report_mutations) => {
            assert_eq!(report_mutations.opt_app_request_id, Some(Uid::from(&[22; UID_LEN])));
            assert_eq!(report_mutations.mutations.len(), 1);
            let report_mutation = &report_mutations.mutations[0];
            match report_mutation {
                NodeReportMutation::Funder(received_funder_report_mutation) => {
                    assert_eq!(received_funder_report_mutation, 
                               &funder_report_mutation);
                },
                _ => unreachable!(),
            }
        },
        _ => unreachable!(),
    }
}

#[test]
fn test_app_server_loop_funder_command() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_app_server_loop_funder_command(thread_pool.clone()));
}

