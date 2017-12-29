#![allow(unused)]
use std::collections::HashMap;

use futures::{Future, Poll, Async, Stream};
use futures::sync::mpsc;
use tokio_core::reactor::Handle;

use inner_messages::{
    IndexingProviderId,
    IndexingProviderStateHash,
    IndexerClientToNetworker,
    NetworkerToIndexerClient,
    FromTimer,
    AppManagerToIndexerClient,
    IndexerClientToAppManager,
    FunderToIndexerClient,
    IndexerClientToFunder,
    DatabaseToIndexerClient,
    IndexerClientToDatabase,
    StateChainLink,
    ResponseSendMessage,
    MessageReceived,
    RequestFriendsRoutes,
};

use schema::indexer::RoutesToIndexer;

use close_handle::CloseHandle;

struct IndexingProviderState {
    id: IndexingProviderId,
    recent_state: StateChainLink,
    routes_to_indexers: RoutesToIndexer,
}

pub struct IndexerClient {
    // The information kept by IndexerClient
    indexing_providers: HashMap<IndexingProviderId, IndexingProviderState>,

    // The senders/receivers to communicate with other modules
    timer_receiver: mpsc::Receiver<FromTimer>,
    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
}

impl IndexerClient {
    fn request_update_providers_state(&self) {
        unimplemented!()
    }

    fn response_send_message(&self, resp: ResponseSendMessage) {
        unimplemented!()
    }

    fn message_received(&self, msg: MessageReceived) {
        // The message type would be received here?
        // 1. RequestUpdateState
        // 2. ResponseNeighborsRoutes
        // 3. ResponseFriendsRoutes
        // TODO: any other message type I missed?
        unimplemented!()
    }

    fn request_friend_routes(&self, req: RequestFriendsRoutes) {
        unimplemented!()
    }

    fn close(&self) {
        unimplemented!()
    }
}

#[derive(Debug)]
pub enum IndexerClientError {}

impl Future for IndexerClient {
    type Item = ();
    type Error = IndexerClientError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // Doing cron
        match self.timer_receiver.poll() {
            Err(_) => {
                error!("timer receiver error, closing");

                self.close();

                return Ok(Async::Ready(()));
            }
            Ok(item) => {
                match item {
                    Async::NotReady => {
                        return Ok(Async::NotReady);
                    },
                    Async::Ready(None) => {
                        error!("timer sender closed, closing");

                        self.close();

                        return Ok(Async::Ready(()));
                    }
                    Async::Ready(Some(FromTimer::TimeTick)) => {
                        // TODO: What kind of work should be done here?
                    }
                }
            }
        }

        match self.networker_receiver.poll() {
            Err(_) => {
                error!("networker receiver error, closing");
            }
            Ok(item) => {
                match item {
                    Async::NotReady => {},
                    Async::Ready(None) => {
                        error!("networker sender closed, closing");
                        self.close();
                        return Ok(Async::Ready(()))
                    }
                    Async::Ready(Some(msg)) => {
                        match msg {
                            NetworkerToIndexerClient::ResponseSendMessage(resp) => {
                                self.response_send_message(resp)
                            },
                            NetworkerToIndexerClient::MessageReceived(msg) => {
                                self.message_received(msg)
                            },
                            NetworkerToIndexerClient::RequestFriendsRoutes(req) => {
                                self.request_friend_routes(req)
                            },
                        }
                    }
                }
            }
        }
        unimplemented!()
    }
}

fn create_indexer_client(
    handle: &Handle,
    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
    timer_receiver: mpsc::Receiver<FromTimer>,
    app_manager_receiver: mpsc::Receiver<AppManagerToIndexerClient>,
    app_manager_sender: mpsc::Sender<IndexerClientToAppManager>,
    funder_receiver: mpsc::Receiver<FunderToIndexerClient>,
    funder_sender: mpsc::Sender<IndexerClientToFunder>,
    database_receiver: mpsc::Receiver<DatabaseToIndexerClient>,
    database_sender: mpsc::Sender<IndexerClientToDatabase>
) -> (CloseHandle, IndexerClient) {
    unimplemented!()
}
