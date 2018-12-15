use futures::executor::ThreadPool;
use futures::channel::{oneshot, mpsc};
use futures::task::{Spawn, SpawnExt, SpawnError};
use futures::{FutureExt, TryFutureExt, StreamExt, SinkExt};

use crate::capacity_graph::{CapacityGraph, CapacityEdge, CapacityRoute};

enum GraphRequest<N,C> {
    /// Change capacities on a directed edge:
    UpdateEdge(N,N,CapacityEdge<C>, oneshot::Sender<Option<CapacityEdge<C>>>),
    /// Remove a directed edge:
    RemoveEdge(N,N, oneshot::Sender<Option<CapacityEdge<C>>>),
    /// Remove a node and all edges starting from this node.
    /// Note: This will not remove edges going to this node.
    RemoveNode(N, oneshot::Sender<bool>),
    /// Get some routes from one node to another of at least certain capacity.
    /// If an exclude directed edge is provided, the routes must not contain this directed edge.
    GetRoutes(N,N,C,Option<(N,N)>, oneshot::Sender<Vec<CapacityRoute<N,C>>>), // (from, to, capacity, opt_exclude)
}

#[derive(Debug)]
pub enum GraphServiceError {
    /// Failed to spawn to self ThreadPool
    LocalSpawnError,
}

/// Util function to convert Option<T> to Vec<T>.
/// Some(t) => vec![t], None => vec![]
fn option_to_vec<T>(opt_t: Option<T>) -> Vec<T> {
    match opt_t {
        Some(t) => vec![t],
        None => vec![],
    }
}

/// Process one GraphRequest, and send the response through the provided sender.
/// This function might perform a long computation and take a long time to complete.
fn process_request<N,C,CG>(capacity_graph: &mut CG, 
                   graph_request: GraphRequest<N,C>) 
where
    CG: CapacityGraph<Node=N, Capacity=C>,
{
    match graph_request {
        GraphRequest::UpdateEdge(a,b,capacity_edge,sender) => {
            let _ = sender.send(capacity_graph.update_edge(a,b,capacity_edge));
        },
        GraphRequest::RemoveEdge(a,b,sender) => {
            let _ = sender.send(capacity_graph.remove_edge(&a,&b));
        },
        GraphRequest::RemoveNode(a,sender) => {
            let _ = sender.send(capacity_graph.remove_node(&a));
        },
        GraphRequest::GetRoutes(a,b,capacity,opt_exclude,sender) => {
            let opt_route = match opt_exclude {
                Some((c,d)) => capacity_graph.get_route(&a,&b,capacity, Some((&c, &d))),
                None => capacity_graph.get_route(&a,&b,capacity, None),
            };
            let routes = option_to_vec(opt_route);
            let _ = sender.send(routes);
        },
    }
}

async fn graph_service_loop<N,C,CG>(mut capacity_graph: CG,
                                    mut incoming_requests: mpsc::Receiver<GraphRequest<N,C>>) 
                                    -> Result<(), GraphServiceError>
where
    N: Send + 'static,
    C: Send + 'static,
    CG: CapacityGraph<Node=N, Capacity=C> + Send + 'static,
{
    // We create our own thread_pool to be used for long graph computations.
    // We don't want to block the external shared thread pool.
    let mut thread_pool = ThreadPool::new()
        .map_err(|_| GraphServiceError::LocalSpawnError)?;

    while let Some(graph_request) = await!(incoming_requests.next()) {
        // Run the graph computation over own pool:
        let process_request_handle = thread_pool.spawn_with_handle(async move {
            process_request(&mut capacity_graph, graph_request);
            capacity_graph
        }).map_err(|_| GraphServiceError::LocalSpawnError)?;

        // Wait for completion of the computation on the external pool:
        capacity_graph = await!(process_request_handle);
    }
    Ok(())
}

pub enum GraphClientError {
    SendRequestError,
    ResponseReceiverClosed,
}


impl From<oneshot::Canceled> for GraphClientError {
    fn from(from: oneshot::Canceled) -> GraphClientError {
        GraphClientError::ResponseReceiverClosed
    }
}

impl From<mpsc::SendError> for GraphClientError {
    fn from(from: mpsc::SendError) -> GraphClientError {
        GraphClientError::SendRequestError
    }
}

#[derive(Clone)]
struct GraphClient<N,C> {
    requests_sender: mpsc::Sender<GraphRequest<N,C>>,
}



impl<N,C> GraphClient<N,C> {
    pub fn new(requests_sender: mpsc::Sender<GraphRequest<N,C>>) -> Self {
        GraphClient {
            requests_sender,
        }
    }

    /// Add or update edge
    pub async fn update_edge(&mut self, a: N, b: N, edge: CapacityEdge<C>) -> Result<Option<CapacityEdge<C>>, GraphClientError> {
        let (sender, receiver) = oneshot::channel::<Option<CapacityEdge<C>>>();
        await!(self.requests_sender.send(GraphRequest::UpdateEdge(a,b,edge,sender)))?;
        Ok(await!(receiver)?)
    }

    /// Remove an edge from the graph
    pub async fn remove_edge(&mut self, a: N, b: N) -> Result<Option<CapacityEdge<C>>, GraphClientError> {
        let (sender, receiver) = oneshot::channel();
        await!(self.requests_sender.send(GraphRequest::RemoveEdge(a,b,sender)))?;
        Ok(await!(receiver)?)
    }

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    /// Returns true if the node `a` was present, false otherwise
    pub async fn remove_node(&mut self, a: N) -> Result<bool, GraphClientError> {
        let (sender, receiver) = oneshot::channel();
        await!(self.requests_sender.send(GraphRequest::RemoveNode(a,sender)))?;
        Ok(await!(receiver)?)
    }

    /// Obtain routes with capacity at least `capacity`. 
    /// Returns each route together with the capacity it is possible to send through that route.
    ///
    /// opt_exclude is an optional edge to exclude (The returned route must not go through this
    /// edge). This can be useful for finding non trivial loops.
    pub async fn get_routes(&mut self, a: N, b: N, capacity: C, opt_exclude: Option<(N, N)>) 
        -> Result<Vec<CapacityRoute<N, C>>, GraphClientError> {

        let (sender, receiver) = oneshot::channel();
        await!(self.requests_sender.send(GraphRequest::GetRoutes(a,b,capacity,opt_exclude,sender)))?;
        Ok(await!(receiver)?)
    }
}

/// Spawn a graph service, returning a GraphClient on success.
/// GraphClient can be cloned to allow multiple clients.
fn create_graph_service<N,C,CG,S>(capacity_graph: CG, 
                                  mut spawner: S) -> Result<GraphClient<N,C>, SpawnError>
where
    N: Send + 'static,
    C: Send + 'static,
    CG: CapacityGraph<Node=N, Capacity=C> + Send + 'static,
    S: Spawn,
{
    let (requests_sender, requests_receiver) = mpsc::channel(0);

    let graph_service_loop_fut = graph_service_loop(
                                    capacity_graph,
                                    requests_receiver)
        .map_err(|e| error!("graph_service_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(graph_service_loop_fut)?;
    Ok(GraphClient::new(requests_sender))
}

