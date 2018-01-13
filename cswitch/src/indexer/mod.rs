pub mod types;
pub mod messages;

//#![allow(unused)]
//use std::mem;
//use std::collections::HashMap;
//
//use futures::{Future, Poll, Async, Stream};
//use futures::sync::{mpsc, oneshot};
//use tokio_core::reactor::Handle;
//
//use inner_messages::{
//    IndexingProviderId,
//    IndexingProviderStateHash,
//    IndexerClientToNetworker,
//    NetworkerToIndexerClient,
//    FromTimer,
//    AppManagerToIndexerClient,
//    IndexerClientToAppManager,
//    FunderToIndexerClient,
//    IndexerClientToFunder,
//    DatabaseToIndexerClient,
//    IndexerClientToDatabase,
//    StateChainLink,
//    ResponseSendMessage,
//    MessageReceived,
//    IndexingProviderInfo,
//    RequestFriendsRoutes,
//    ResponseFriendsRoutes,
//    RequestNeighborsRoutes,
//    ResponseNeighborsRoutes,
//};
//
////use service::ServiceState;
//use schema::indexer::RoutesToIndexer;
//
//use close_handle::CloseHandle;
//
//struct IndexingProviderState {
//    recent_state: StateChainLink,
//    routes_to_indexers: Vec<RoutesToIndexer>,
//}
//
//pub struct IndexerClient {
//    state: ServiceState<IndexerClientError>,
//
//    // The information kept by IndexerClient
//    indexing_providers: HashMap<IndexingProviderId, IndexingProviderState>,
//
//    // The senders/receivers to communicate with other modules
//    timer_receiver: mpsc::Receiver<FromTimer>,
//    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
//    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
//    app_manager_receiver: mpsc::Receiver<AppManagerToIndexerClient>,
//    app_manager_sender: mpsc::Sender<IndexerClientToAppManager>,
//    funder_receiver: mpsc::Receiver<FunderToIndexerClient>,
//    funder_sender: mpsc::Sender<IndexerClientToFunder>,
//    database_receiver: mpsc::Receiver<DatabaseToIndexerClient>,
//    database_sender: mpsc::Sender<IndexerClientToDatabase>,
//
//    // For reporting the closing event
//    close_sender:   Option<oneshot::Sender<()>>,
//    // For observing the closing request
//    close_receiver: oneshot::Receiver<()>,
//}
//
//impl IndexerClient {
//    fn response_send_message(&self, resp: ResponseSendMessage) {
//        unimplemented!()
//    }
//
//    fn message_received(&self, msg: MessageReceived) {
//        // The message type would be received here?
//        // 1. RequestUpdateState
//        // 2. ResponseNeighborsRoutes
//        // 3. ResponseFriendsRoutes
//        // 4. RoutesToIndexer
//        // TODO: any other message type I missed?
//        unimplemented!()
//    }
//
//    fn request_friends_routes(&self, req: RequestFriendsRoutes) {
//        unimplemented!()
//    }
//
//    fn request_neighbors_routes(&self, req: RequestNeighborsRoutes) {
//        unimplemented!()
//    }
//
//    fn add_indexing_provider(&mut self, info: IndexingProviderInfo) {
//        let indexing_provider_id = info.id;
//        self.indexing_providers.insert(
//            indexing_provider_id,
//            IndexingProviderState {
//                recent_state: info.state_chain_link,
//                routes_to_indexers: Vec::new(),
//            }
//        );
//    }
//
//    fn del_indexing_provider(&mut self, id: IndexingProviderId) {
//        self.indexing_providers.remove(&id);
//    }
//
//    fn close(&mut self) {
//        self.timer_receiver.close();
//        self.networker_receiver.close();
//        self.app_manager_receiver.close();
//        self.funder_receiver.close();
//        self.database_receiver.close();
//    }
//}
//
//#[derive(Debug)]
//pub enum IndexerClientError {}
//
//impl Future for IndexerClient {
//    type Item = ();
//    type Error = IndexerClientError;
//
//    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//        // TODO: Use macro to avoid duplication of code
//        loop {
//            match mem::replace(&mut self.state, ServiceState::Empty) {
//                ServiceState::Empty => unreachable!("something go wrong"),
//                ServiceState::Closing(closing_fut) => {}
//                ServiceState::Living => {
//                    // Doing cron
//                    match self.timer_receiver.poll() {
//                        Err(_) => {
//                            error!("timer receiver error, closing");
//                            self.close();
//                        }
//                        Ok(item) => {
//                            match item {
//                                Async::NotReady => {
//                                    return Ok(Async::NotReady);
//                                },
//                                Async::Ready(None) => {
//                                    error!("timer sender closed, closing");
//                                    self.close();
//                                    return Ok(Async::Ready(()));
//                                }
//                                Async::Ready(Some(FromTimer::TimeTick)) => {
//                                    // TODO: What kind of work should be done here?
//                                }
//                            }
//                        }
//                    }
//
//                    match self.networker_receiver.poll() {
//                        Err(_) => {
//                            error!("networker receiver error, closing");
//                        }
//                        Ok(item) => {
//                            match item {
//                                Async::NotReady => {},
//                                Async::Ready(None) => {
//                                    error!("networker sender closed, closing");
//                                    self.close();
//                                    return Ok(Async::Ready(()))
//                                }
//                                Async::Ready(Some(msg)) => {
//                                    match msg {
//                                        NetworkerToIndexerClient::ResponseSendMessage(resp) => {
//                                            self.response_send_message(resp)
//                                        },
//                                        NetworkerToIndexerClient::MessageReceived(msg) => {
//                                            self.message_received(msg)
//                                        },
//                                        NetworkerToIndexerClient::RequestFriendsRoutes(req) => {
//                                            self.request_friends_routes(req)
//                                        },
//                                    }
//                                }
//                            }
//                        }
//                    }
//
//                    match self.app_manager_receiver.poll() {
//                        Err(_) => {
//                            error!("app manager receiver error, closing");
//                        }
//                        Ok(item) => {
//                            match item {
//                                Async::NotReady => {},
//                                Async::Ready(None) => {
//                                    error!("app_manager sender closed, closing");
//                                    self.close();
//                                    return Ok(Async::Ready(()))
//                                }
//                                Async::Ready(Some(msg)) => {
//                                    match msg {
//                                        AppManagerToIndexerClient::SetIndexingProviderStatus { .. } => {
//                                            // TODO
//                                            assert!(false);
//                                        },
//                                        AppManagerToIndexerClient::AddIndexingProvider(info) => {
//                                            self.add_indexing_provider(info);
//                                        }
//                                        AppManagerToIndexerClient::RemoveIndexingProvider { id } => {
//                                            self.del_indexing_provider(id);
//                                        }
//                                        AppManagerToIndexerClient::RequestFriendsRoutes(req) => {
//                                            self.request_friends_routes(req);
//                                        }
//                                        AppManagerToIndexerClient::RequestNeighborsRoutes(req) => {
//                                            self.request_neighbors_routes(req);
//                                        }
//                                    }
//                                }
//                            }
//                        }
//                    }
//
//                    match self.funder_receiver.poll() {
//                        Err(_) => {
//                            error!("app manager receiver error, closing");
//                        }
//                        Ok(item) => {
//                            match item {
//                                Async::NotReady => {},
//                                Async::Ready(None) => {
//                                    error!("app_manager sender closed, closing");
//                                    self.close();
//                                    return Ok(Async::Ready(()))
//                                }
//                                Async::Ready(Some(msg)) => {
//                                    match msg {
//                                        FunderToIndexerClient::RequestNeighborsRoute(req) => {
//                                            self.request_neighbors_routes(req);
//                                        }
//                                    }
//                                }
//                            }
//                        }
//                    }
//                }
//            }
//        }
//
//        unimplemented!()
//    }
//}
//
//fn create_indexer_client(
//    handle: &Handle,
//    networker_sender: mpsc::Sender<IndexerClientToNetworker>,
//    networker_receiver: mpsc::Receiver<NetworkerToIndexerClient>,
//    timer_receiver: mpsc::Receiver<FromTimer>,
//    app_manager_receiver: mpsc::Receiver<AppManagerToIndexerClient>,
//    app_manager_sender: mpsc::Sender<IndexerClientToAppManager>,
//    funder_receiver: mpsc::Receiver<FunderToIndexerClient>,
//    funder_sender: mpsc::Sender<IndexerClientToFunder>,
//    database_receiver: mpsc::Receiver<DatabaseToIndexerClient>,
//    database_sender: mpsc::Sender<IndexerClientToDatabase>
//) -> (CloseHandle, IndexerClient) {
//    unimplemented!()
//}
