use std::mem;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, SinkExt, Stream, StreamExt};

use common::conn::BoxStream;
use common::select_streams::select_streams;

use stcompact::compact_node::messages::{CompactToUserAck, CompactToUser, CompactReport};


#[derive(Debug)]
struct CompactReportRequest {
    response_sender: oneshot::Sender<CompactReport>,
}

#[derive(Debug, Clone)]
pub struct CompactReportClient {
    requests_sender: mpsc::Sender<CompactReportRequest>,
}

impl CompactReportClient {
    fn new(requests_sender: mpsc::Sender<CompactReportRequest>) -> Self {
        CompactReportClient { requests_sender }
    }

    pub async fn request_report(&mut self) -> CompactReport {
        let (response_sender, response_receiver) = oneshot::channel();
        let report_request = CompactReportRequest { response_sender };
        self.requests_sender.send(report_request).await.unwrap();

        response_receiver.await.unwrap()
    }
}

#[derive(Debug)]
enum CompactReportServiceEvent {
    Request(CompactReportRequest),
    CompactToUserAck(CompactToUserAck),
    ServerClosed,
}

const REPORT_CHANNEL_LEN: usize = 0x200;

/// A service for maintaining knowledge of the current report
pub fn compact_report_service<S, FS>(
    mut compact_report: CompactReport,
    from_server: FS,
    spawner: &S,
) -> (mpsc::Receiver<CompactToUserAck>, CompactReportClient)
where
    S: Spawn,
    FS: Stream<Item = CompactToUserAck> + Unpin + Send + 'static,
{
    let (requests_sender, requests_receiver) = mpsc::channel(1);
    let (mut app_sender, app_receiver) = mpsc::channel(REPORT_CHANNEL_LEN);

    let requests_receiver = requests_receiver.map(CompactReportServiceEvent::Request);
    let from_server = from_server
        .map(CompactReportServiceEvent::CompactToUserAck)
        .chain(stream::once(future::ready(
            CompactReportServiceEvent::ServerClosed,
        )));

    let mut incoming_events = select_streams![from_server, requests_receiver];

    spawner
        .spawn(async move {
            while let Some(incoming_event) = incoming_events.next().await {
                match incoming_event {
                    CompactReportServiceEvent::Request(report_request) => {
                        report_request.response_sender.send(compact_report.clone()).unwrap();
                    }
                    CompactReportServiceEvent::CompactToUserAck(compact_to_user_ack) => {
                        if let CompactToUserAck::CompactToUser(CompactToUser::Report(new_compact_report)) = 
                            &compact_to_user_ack
                        {
                            let _ = mem::replace(&mut compact_report, new_compact_report.clone());
                        }
                        let _ = app_sender.send(compact_to_user_ack).await;
                    }
                    CompactReportServiceEvent::ServerClosed => {
                        return;
                    }
                }
            }
        })
        .unwrap();

    (app_receiver, CompactReportClient::new(requests_sender))
}
