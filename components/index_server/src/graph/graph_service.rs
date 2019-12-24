use std::collections::HashMap;
use std::hash::Hash;

use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnError, SpawnExt};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};

use super::capacity_graph::{CapacityEdge, CapacityGraph, CapacityMultiRoute};

pub enum GraphRequest<G, N, C, T> {
    /// Change capacities on a directed edge:
    UpdateEdge(
        G,
        N,
        N,
        CapacityEdge<C, T>,
        oneshot::Sender<Option<CapacityEdge<C, T>>>,
    ),
    /// Remove a directed edge:
    RemoveEdge(G, N, N, oneshot::Sender<Option<CapacityEdge<C, T>>>),
    /// Remove a node and all edges starting from this node.
    /// Note: This will not remove edges going to this node.
    RemoveNode(N, oneshot::Sender<()>),
    /// Get some routes from one node to another of at least certain capacity.
    /// If an exclude directed edge is provided, the routes must not contain this directed edge.
    GetMultiRoutes(
        G,
        N,
        N,
        C,
        Option<(N, N)>,
        oneshot::Sender<Vec<CapacityMultiRoute<N, C, T>>>,
    ), // (from, to, capacity, opt_exclude)
    /// Expire old outgoing edges for the specified node
    Tick(N, oneshot::Sender<()>),
}

#[derive(Debug)]
pub enum GraphServiceError {
    /// Failed to spawn to self ThreadPool
    LocalSpawnError,
}

#[allow(clippy::many_single_char_names)]
/// Process one GraphRequest, and send the response through the provided sender.
/// This function might perform a long computation and take a long time to complete.
fn process_request<G, N, C, T, CG>(
    capacity_graphs: &mut HashMap<G, CG>,
    graph_request: GraphRequest<G, N, C, T>,
) where
    G: Hash + Eq,
    CG: CapacityGraph<Node = N, Capacity = C, Rate = T>,
{
    match graph_request {
        GraphRequest::UpdateEdge(g, a, b, capacity_edge, sender) => {
            let capacity_graph = capacity_graphs.entry(g).or_insert_with(CG::new);
            let _ = sender.send(capacity_graph.update_edge(a, b, capacity_edge));
        }
        GraphRequest::RemoveEdge(g, a, b, sender) => {
            if let Some(capacity_graph) = capacity_graphs.get_mut(&g) {
                let _ = sender.send(capacity_graph.remove_edge(&a, &b));
            }
        }
        GraphRequest::RemoveNode(a, sender) => {
            capacity_graphs.retain(|_g, capacity_graph| capacity_graph.remove_node(&a));
            let _ = sender.send(());
        }
        GraphRequest::GetMultiRoutes(g, a, b, capacity, opt_exclude, sender) => {
            let routes = if let Some(capacity_graph) = capacity_graphs.get_mut(&g) {
                match opt_exclude {
                    Some((c, d)) => {
                        capacity_graph.get_multi_routes(&a, &b, capacity, Some((&c, &d)))
                    }
                    None => capacity_graph.get_multi_routes(&a, &b, capacity, None),
                }
            } else {
                vec![]
            };
            let _ = sender.send(routes);
        }
        GraphRequest::Tick(a, sender) => {
            for capacity_graph in capacity_graphs.values_mut() {
                capacity_graph.tick(&a);
            }
            let _ = sender.send(());
        }
    }
}

async fn graph_service_loop<G, N, C, T, CG, GS>(
    mut capacity_graphs: HashMap<G, CG>,
    mut incoming_requests: mpsc::Receiver<GraphRequest<G, N, C, T>>,
    graph_service_spawner: GS,
) -> Result<(), GraphServiceError>
where
    G: Send + Hash + Eq + 'static,
    N: Send + 'static,
    C: Send + 'static,
    T: Send + 'static,
    CG: CapacityGraph<Node = N, Capacity = C, Rate = T> + Send + 'static,
    GS: Spawn,
{
    // We use a separate spawner to be used for long graph computations.
    // We don't want to block the external shared thread pool.

    while let Some(graph_request) = incoming_requests.next().await {
        // Run the graph computation over own pool:
        let process_request_handle = graph_service_spawner
            .spawn_with_handle(async move {
                process_request(&mut capacity_graphs, graph_request);
                capacity_graphs
            })
            .map_err(|_| GraphServiceError::LocalSpawnError)?;

        // Wait for completion of the computation on the external pool:
        capacity_graphs = process_request_handle.await;
    }
    Ok(())
}

#[derive(Debug)]
pub enum GraphClientError {
    SendRequestError,
    ResponseReceiverClosed,
}

impl From<oneshot::Canceled> for GraphClientError {
    fn from(_from: oneshot::Canceled) -> GraphClientError {
        GraphClientError::ResponseReceiverClosed
    }
}

impl From<mpsc::SendError> for GraphClientError {
    fn from(_from: mpsc::SendError) -> GraphClientError {
        GraphClientError::SendRequestError
    }
}

#[derive(Clone)]
pub struct GraphClient<G, N, C, T> {
    requests_sender: mpsc::Sender<GraphRequest<G, N, C, T>>,
}

impl<G, N, C, T> GraphClient<G, N, C, T> {
    pub fn new(requests_sender: mpsc::Sender<GraphRequest<G, N, C, T>>) -> Self {
        GraphClient { requests_sender }
    }

    /// Add or update edge
    pub async fn update_edge(
        &mut self,
        g: G,
        a: N,
        b: N,
        capacity_edge: CapacityEdge<C, T>,
    ) -> Result<Option<CapacityEdge<C, T>>, GraphClientError> {
        let (sender, receiver) = oneshot::channel::<Option<CapacityEdge<C, T>>>();
        self.requests_sender
            .send(GraphRequest::UpdateEdge(g, a, b, capacity_edge, sender))
            .await?;
        Ok(receiver.await?)
    }

    /// Remove an edge from the graph
    pub async fn remove_edge(
        &mut self,
        g: G,
        a: N,
        b: N,
    ) -> Result<Option<CapacityEdge<C, T>>, GraphClientError> {
        let (sender, receiver) = oneshot::channel();
        self.requests_sender
            .send(GraphRequest::RemoveEdge(g, a, b, sender))
            .await?;
        Ok(receiver.await?)
    }

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    pub async fn remove_node(&mut self, a: N) -> Result<(), GraphClientError> {
        let (sender, receiver) = oneshot::channel();

        self.requests_sender
            .send(GraphRequest::RemoveNode(a, sender))
            .await?;

        Ok(receiver.await?)
    }

    /// Obtain routes with capacity at least `capacity`.
    /// Returns each route together with the capacity it is possible to send through that route.
    ///
    /// opt_exclude is an optional edge to exclude (The returned route must not go through this
    /// edge). This can be useful for finding non trivial loops.
    pub async fn get_multi_routes(
        &mut self,
        g: G,
        a: N,
        b: N,
        capacity: C,
        opt_exclude: Option<(N, N)>,
    ) -> Result<Vec<CapacityMultiRoute<N, C, T>>, GraphClientError> {
        let (sender, receiver) = oneshot::channel();
        self.requests_sender
            .send(GraphRequest::GetMultiRoutes(
                g,
                a,
                b,
                capacity,
                opt_exclude,
                sender,
            ))
            .await?;
        Ok(receiver.await?)
    }

    /// Remove an edge from the graph
    pub async fn tick(&mut self, a: N) -> Result<(), GraphClientError> {
        let (sender, receiver) = oneshot::channel();
        self.requests_sender
            .send(GraphRequest::Tick(a, sender))
            .await?;
        Ok(receiver.await?)
    }
}

/// Spawn a graph service, returning a GraphClient on success.
/// GraphClient can be cloned to allow multiple clients.
pub fn create_graph_service<G, N, C, T, CG, GS, S>(
    graph_service_spawner: GS,
    spawner: S,
) -> Result<GraphClient<G, N, C, T>, SpawnError>
where
    G: Hash + Eq + Send + 'static,
    N: Send + 'static,
    C: Send + 'static,
    T: Send + 'static,
    CG: CapacityGraph<Node = N, Capacity = C, Rate = T> + Send + 'static,
    GS: Spawn + Send + 'static,
    S: Spawn,
{
    let (requests_sender, requests_receiver) = mpsc::channel(0);

    let capacity_graphs = HashMap::<G, CG>::new();

    let graph_service_loop_fut =
        graph_service_loop(capacity_graphs, requests_receiver, graph_service_spawner)
            .map_err(|e| error!("graph_service_loop() error: {:?}", e))
            .map(|_| ());

    spawner.spawn(graph_service_loop_fut)?;
    Ok(GraphClient::new(requests_sender))
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::capacity_graph::CapacityRoute;
    use super::super::simple_capacity_graph::SimpleCapacityGraph;
    use super::super::test_utils::ConstRate;

    use futures::executor::{block_on, ThreadPool};

    async fn task_create_graph_service_basic<S>(spawner: S)
    where
        S: Spawn,
    {
        // We use currency as the G type. Every currency should have its own graph:
        let currency1 = 1u8;

        let graph_service_spawner = ThreadPool::new().unwrap();
        let mut graph_client = create_graph_service::<
            u8,
            u32,
            u128,
            ConstRate,
            SimpleCapacityGraph<u32, ConstRate>,
            _,
            _,
        >(graph_service_spawner, spawner)
        .unwrap();

        graph_client
            .update_edge(
                currency1,
                2u32,
                5u32,
                CapacityEdge::new(true, 5, ConstRate(1u32)),
            )
            .await
            .unwrap();
        graph_client
            .update_edge(currency1, 5, 2, CapacityEdge::new(true, 30, ConstRate(1)))
            .await
            .unwrap();

        assert_eq!(
            graph_client
                .get_multi_routes(currency1, 2, 5, 29, None)
                .await
                .unwrap(),
            vec![CapacityMultiRoute {
                routes: vec![CapacityRoute {
                    route: vec![2, 5],
                    capacity: 30,
                    rate: ConstRate(0),
                }],
            }]
        );
        assert_eq!(
            graph_client
                .get_multi_routes(currency1, 2, 5, 30, None)
                .await
                .unwrap(),
            vec![CapacityMultiRoute {
                routes: vec![CapacityRoute {
                    route: vec![2, 5],
                    capacity: 30,
                    rate: ConstRate(0),
                }],
            }]
        );
        assert_eq!(
            graph_client
                .get_multi_routes(currency1, 2, 5, 31, None)
                .await
                .unwrap(),
            vec![]
        );

        graph_client.tick(2).await.unwrap();

        assert_eq!(
            graph_client.remove_edge(currency1, 2, 5).await.unwrap(),
            Some(CapacityEdge::new(true, 5, ConstRate(1)))
        );
        graph_client.remove_node(2).await.unwrap();
        graph_client.remove_node(5).await.unwrap();
    }

    #[test]
    fn test_create_graph_service_basic() {
        let thread_pool = ThreadPool::new().unwrap();

        block_on(task_create_graph_service_basic(thread_pool.clone()));
    }
}

// TODO: Add a test for multiple currencies at the same time (Different values for the G type)
