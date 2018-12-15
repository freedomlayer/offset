
pub type CapacityEdge<C> = (C, C);
pub type CapacityRoute<N,C> = (Vec<N>, C);

pub trait CapacityGraph {
    type Node;      // Node type   
    type Capacity;  // Directed capacity between two neighboring nodes

    /// Add or update edge
    fn update_edge(&mut self, a: Self::Node, b: Self::Node, edge: CapacityEdge<Self::Capacity>) -> Option<CapacityEdge<Self::Capacity>>;

    /// Remove an edge from the graph
    fn remove_edge(&mut self, a: &Self::Node, b: &Self::Node) -> Option<CapacityEdge<Self::Capacity>>;

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    /// Returns true if the node `a` was present, false otherwise
    fn remove_node(&mut self, a: &Self::Node) -> bool;

    /// Get a route with capacity at least `capacity`. 
    /// Returns the route together with the capacity it is possible to send through the route.
    ///
    /// opt_exclude is an optional edge to exclude (The returned route must not go through this
    /// edge). This can be useful for finding non trivial loops.
    fn get_routes(&self, a: &Self::Node, b: &Self::Node, capacity: Self::Capacity, opt_exclude: Option<(&Self::Node, &Self::Node)>) 
        -> Vec<CapacityRoute<Self::Node, Self::Capacity>>;

}
