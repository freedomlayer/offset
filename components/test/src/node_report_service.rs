
use futures::channel::{mpsc, oneshot};

use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, FutureExt, SinkExt, Stream, StreamExt};








use common::conn::{BoxFuture, BoxStream, ConnPair};
use common::select_streams::select_streams;







use app::conn::AppServerToApp;
use app::report::NodeReport;












#[derive(Debug)]
struct NodeReportRequest {
    response_sender: oneshot::Sender<NodeReport>,
}

#[derive(Debug, Clone)]
pub struct NodeReportClient {
    requests_sender: mpsc::Sender<NodeReportRequest>,
}

impl NodeReportClient {
    fn new(requests_sender: mpsc::Sender<NodeReportRequest>) -> Self {
        NodeReportClient { requests_sender }
    }

    pub async fn request_report(&mut self) -> NodeReport {
        let (response_sender, response_receiver) = oneshot::channel();
        let report_request = NodeReportRequest { response_sender };
        self.requests_sender.send(report_request).await.unwrap();

        response_receiver.await.unwrap()
    }
}

#[derive(Debug)]
enum NodeReportServiceEvent {
    Request(NodeReportRequest),
    AppServerToApp(AppServerToApp),
    ServerClosed,
}

const APP_SERVER_TO_APP_CHANNEL_LEN: usize = 0x200;

/// A service for maintaining knowledge of the current report
pub fn node_report_service<S, FS>(
    mut node_report: NodeReport,
    mut from_server: FS,
    spawner: &S,
) -> (mpsc::Receiver<AppServerToApp>, NodeReportClient)
where
    S: Spawn,
    FS: Stream<Item = AppServerToApp> + Unpin + Send + 'static,
{
    let (requests_sender, requests_receiver) = mpsc::channel(1);
    let (mut app_sender, app_receiver) = mpsc::channel(APP_SERVER_TO_APP_CHANNEL_LEN);

    let requests_receiver = requests_receiver.map(NodeReportServiceEvent::Request);
    let from_server = from_server
        .map(NodeReportServiceEvent::AppServerToApp)
        .chain(stream::once(future::ready(
            NodeReportServiceEvent::ServerClosed,
        )));

    let mut incoming_events = select_streams![from_server, requests_receiver];

    spawner
        .spawn(async move {
            while let Some(incoming_event) = incoming_events.next().await {
                match incoming_event {
                    NodeReportServiceEvent::Request(report_request) => {
                        report_request.response_sender.send(node_report.clone());
                    }
                    NodeReportServiceEvent::AppServerToApp(app_server_to_app) => {
                        if let AppServerToApp::ReportMutations(report_mutations) =
                            &app_server_to_app
                        {
                            for mutation in &report_mutations.mutations {
                                node_report.mutate(&mutation).unwrap();
                            }
                        }
                        let _ = app_sender.send(app_server_to_app).await;
                    }
                    NodeReportServiceEvent::ServerClosed => {
                        return;
                    }
                }
            }
        })
        .unwrap();

    (app_receiver, NodeReportClient { requests_sender })
}
