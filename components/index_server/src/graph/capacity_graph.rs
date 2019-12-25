// pub type CapacityPair<C> = (C, C);

pub trait LinearRate
where
    Self: std::marker::Sized,
{
    /// Type used to count credits
    type K;

    /// The zero LinearRate:
    fn zero() -> Self;
    /// Calculate the fee for forwarding a certain amount of credits being passed.
    /// The resulting fee is also an amount of credits.
    fn calc_fee(&self, k: Self::K) -> Option<Self::K>;
    /// Attempt to add two rates.
    fn checked_add(&self, other: &Self) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityEdge<C, T> {
    pub recv_capacity: C,
    pub rate: T,
}

impl<C, T> CapacityEdge<C, T> {
    pub fn new(recv_capacity: C, rate: T) -> Self {
        Self {
            recv_capacity,
            rate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityRoute<N, C, T> {
    pub route: Vec<N>,
    pub capacity: C,
    pub rate: T,
}

#[derive(Debug, Eq, PartialEq)]
pub struct CapacityMultiRoute<N, C, T> {
    pub routes: Vec<CapacityRoute<N, C, T>>,
}

pub trait CapacityGraph {
    type Node; // Node type
    type Capacity; // Directed capacity between two neighboring nodes
    type Rate; // A type for the rate of sending credits along an edge/route.

    /// Create a new empty CapacityGraph
    fn new() -> Self;

    /// Add or update edge
    fn update_edge(
        &mut self,
        a: Self::Node,
        b: Self::Node,
        capacity_edge: CapacityEdge<Self::Capacity, Self::Rate>,
    ) -> Option<CapacityEdge<Self::Capacity, Self::Rate>>;

    /// Remove an edge from the graph
    fn remove_edge(
        &mut self,
        a: &Self::Node,
        b: &Self::Node,
    ) -> Option<CapacityEdge<Self::Capacity, Self::Rate>>;

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    /// Returns true if the CapacityGraph is now empty
    fn remove_node(&mut self, a: &Self::Node) -> bool;

    /// Get a multi routes with capacity at least `capacity`.
    /// Returns every route in the multi route with the following additional information:
    /// - Capacity (Amount of credits we can push along that route)
    /// - Rate: Aggregated rate of how much it costs to send credits along that route.
    ///
    /// opt_exclude is an optional edge to exclude (All of the returned routes must not go through this
    /// edge). This can be useful for finding non trivial loops.
    fn get_multi_routes(
        &self,
        a: &Self::Node,
        b: &Self::Node,
        capacity: Self::Capacity,
        opt_exclude: Option<(&Self::Node, &Self::Node)>,
    ) -> Vec<CapacityMultiRoute<Self::Node, Self::Capacity, Self::Rate>>;

    /// Simulate advancement of time. Used to remove old edges.
    fn tick(&mut self, a: &Self::Node);
}
