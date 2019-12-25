use std::collections::HashMap;
use std::{cmp, hash};

use super::bfs::bfs;
use super::capacity_graph::{
    CapacityEdge, CapacityGraph, CapacityMultiRoute, CapacityRoute, LinearRate,
};
use super::utils::{option_to_vec, OptionIterator};

/// Amount of ticks an edge could live regardless of coupon collector's approximation.
/// This is useful to allow the first edges build (n*log(n) is very small for small n).
const BASE_MAX_EDGE_AGE: u128 = 16;

#[derive(Debug, Clone)]
struct Edge<T> {
    capacity_edge: CapacityEdge<u128, T>,
    age: u128,
}

impl<T> Edge<T> {
    fn new(capacity_edge: CapacityEdge<u128, T>) -> Self {
        Edge {
            capacity_edge,
            age: 0,
        }
    }
}

struct NodeEdges<N, T> {
    edges: HashMap<N, Edge<T>>,
}

impl<N, T> NodeEdges<N, T>
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
{
    fn new() -> Self {
        NodeEdges {
            edges: HashMap::new(),
        }
    }
}

/// The client sends us information about his neighbors in a cyclic fashion.
/// This means that after about `N` messages a complete cycle will be completed
/// and we will obtain information about the full client state.
///
/// We are being generous and willing to wait about `Const + 3 * N`
/// until an edge is removed.
fn max_edge_age(num_edges: usize) -> u128 {
    BASE_MAX_EDGE_AGE + 3 * (num_edges as u128)
}

impl<N, T> NodeEdges<N, T>
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
{
    pub fn tick(&mut self) {
        let max_edge_age = max_edge_age(self.edges.len());

        self.edges.retain(|_remote_node, edge| {
            edge.age = edge.age.saturating_add(1);
            edge.age < max_edge_age
        });
    }
}

pub struct SimpleCapacityGraph<N, T> {
    nodes: HashMap<N, NodeEdges<N, T>>,
}

impl<N, T> SimpleCapacityGraph<N, T>
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
    T: LinearRate + Clone,
{
    pub fn new() -> SimpleCapacityGraph<N, T> {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Get a directed edge (if exists)
    fn get_edge(&self, a: &N, b: &N) -> Option<Edge<T>> {
        match self.nodes.get(a) {
            None => None,
            Some(a_edges) => match a_edges.edges.get(b) {
                None => None,
                Some(a_b_edge) => Some(a_b_edge.clone()),
            },
        }
    }

    /// Get the send capacity from `a` to a direct neighbor `b`.
    fn get_send_capacity(&self, a: &N, b: &N) -> u128 {
        if self.get_edge(&a, &b).is_none() {
            return 0;
        }

        if let Some(b_a_edge) = self.get_edge(&b, &a) {
            b_a_edge.capacity_edge.recv_capacity
        } else {
            0
        }
    }

    fn neighbors_with_send_capacity(
        &self,
        a: N,
        capacity: u128,
    ) -> OptionIterator<impl Iterator<Item = &N>> {
        let a_edges = match self.nodes.get(&a) {
            Some(a_edges) => a_edges,
            None => return OptionIterator::new(None),
        };
        let iter = a_edges
            .edges
            .keys()
            .filter(move |b| self.get_send_capacity(&a, b) >= capacity);
        OptionIterator::new(Some(iter))
    }

    /// Calculate the amount of capacity we can send through a route.
    /// This amount if the minimum of all edge capacities of the route.
    fn get_route_capacity(&self, route: &[N]) -> Option<u128> {
        (0..route.len().checked_sub(1)?)
            .map(|i| self.get_send_capacity(&route[i], &route[i + 1]))
            .min()
    }

    /// Calculate the total rate of sending credits along a given route
    fn get_route_rate(&self, route: &[N]) -> Option<T> {
        let mut total_rate = T::zero();
        // If the route is only of length 2, the rate will be 0.
        // No fees are paid for the last hop. TODO: Is this the right behaviour?
        for i in 0..route.len().checked_sub(2)? {
            let edge = self.get_edge(&route[i], &route[i + 1])?;
            total_rate = total_rate.checked_add(&edge.capacity_edge.rate)?;
        }
        Some(total_rate)
    }

    /// Get a route with capacity at least `capacity`.
    /// Returns the route together with the capacity it is possible to send through the route.
    ///
    /// opt_exclude is an optional edge to exclude (The returned route must not go through this
    /// edge). This can be useful for finding non trivial loops.
    fn get_multi_route(
        &self,
        a: &N,
        b: &N,
        capacity: u128,
        opt_exclude: Option<(&N, &N)>,
    ) -> Option<CapacityMultiRoute<N, u128, T>> {
        // TODO: Update this implementation:
        // Currently get_route does not attemp to find the cheapest route (according to rate)
        // It only finds the shortest route and then calculates the rate.

        let (opt_e_start, opt_e_end) = match opt_exclude {
            Some((e_start, e_end)) => (Some(e_start), Some(e_end)),
            None => (None, None),
        };
        let get_neighbors = |cur_node: &N| {
            let cur_node_is_e_start = Some(cur_node) == opt_e_start;
            self.neighbors_with_send_capacity(cur_node.clone(), capacity)
                .filter(move |&next_node| !cur_node_is_e_start || Some(next_node) != opt_e_end)
        };
        let route = bfs(a, b, get_neighbors)?;
        // We assert that we will always have valid capacity here:
        let capacity = self.get_route_capacity(&route).unwrap();

        let rate = self.get_route_rate(&route)?;

        let graph_route = CapacityRoute {
            route,
            capacity,
            rate,
        };

        Some(CapacityMultiRoute {
            routes: vec![graph_route],
        })
    }
}

impl<N, T> CapacityGraph for SimpleCapacityGraph<N, T>
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
    T: LinearRate + Clone,
{
    type Node = N;
    type Capacity = u128;
    type Rate = T;

    fn new() -> Self {
        SimpleCapacityGraph::new()
    }

    /// Add or update edge
    fn update_edge(
        &mut self,
        a: N,
        b: N,
        capacity_edge: CapacityEdge<u128, T>,
    ) -> Option<CapacityEdge<u128, T>> {
        let a_entry = self.nodes.entry(a).or_insert_with(NodeEdges::new);
        a_entry
            .edges
            .insert(b, Edge::new(capacity_edge))
            .map(|edge| edge.capacity_edge)
    }

    /// Remove an edge from the graph
    fn remove_edge(&mut self, a: &N, b: &N) -> Option<CapacityEdge<u128, T>> {
        let a_edges = match self.nodes.get_mut(a) {
            Some(a_edges) => a_edges,
            None => return None,
        };

        let old_edge = match a_edges.edges.remove(b) {
            Some(edge) => edge,
            None => return None,
        };

        if a_edges.edges.is_empty() {
            self.nodes.remove(a);
        }

        Some(old_edge.capacity_edge)
    }

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    /// Returns true if the SimpleCapacityGraph is now empty.
    fn remove_node(&mut self, a: &N) -> bool {
        let _ = self.nodes.remove(a);
        self.nodes.is_empty()
    }

    fn get_multi_routes(
        &self,
        a: &N,
        b: &N,
        capacity: u128,
        opt_exclude: Option<(&N, &N)>,
    ) -> Vec<CapacityMultiRoute<N, u128, T>> {
        option_to_vec(self.get_multi_route(a, b, capacity, opt_exclude))
    }

    fn tick(&mut self, a: &N) {
        if let Some(node_edges) = self.nodes.get_mut(a) {
            node_edges.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::test_utils::ConstRate;

    #[test]
    fn test_get_send_capacity_basic() {
        let mut cg = SimpleCapacityGraph::<u32, ConstRate>::new();
        cg.update_edge(0, 1, CapacityEdge::new(20, ConstRate(1)));
        cg.update_edge(1, 0, CapacityEdge::new(5, ConstRate(1)));

        assert_eq!(cg.get_send_capacity(&0, &1), 5);
        assert_eq!(cg.get_send_capacity(&1, &0), 20);
    }

    #[test]
    fn test_get_send_capacity_one_sided() {
        let mut cg = SimpleCapacityGraph::<u32, ConstRate>::new();
        cg.update_edge(0, 1, CapacityEdge::new(20, ConstRate(1)));

        assert_eq!(cg.get_send_capacity(&0, &1), 0);
        assert_eq!(cg.get_send_capacity(&1, &0), 0);
    }

    #[test]
    fn test_add_remove_edge() {
        let mut cg = SimpleCapacityGraph::<u32, ConstRate>::new();
        assert_eq!(cg.remove_edge(&0, &1), None);
        cg.update_edge(0, 1, CapacityEdge::new(20, ConstRate(1)));
        assert_eq!(cg.nodes.len(), 1);

        assert_eq!(
            cg.remove_edge(&0, &1),
            Some(CapacityEdge::new(20, ConstRate(1)))
        );
        assert_eq!(cg.nodes.len(), 0);

        cg.update_edge(0, 1, CapacityEdge::new(20, ConstRate(1)));
        assert_eq!(cg.nodes.len(), 1);
        cg.remove_node(&1);
        assert_eq!(cg.nodes.len(), 1);
    }

    fn example_capacity_graph() -> SimpleCapacityGraph<u32, ConstRate> {
        /*
         * Example graph:
         *
         *    0 --> 1 --> 2 --> 5
         *          |     ^
         *          V     |
         *          3 --> 4
         *
         */

        let mut cg = SimpleCapacityGraph::<u32, ConstRate>::new();

        cg.update_edge(0, 1, CapacityEdge::new(10, ConstRate(1)));
        cg.update_edge(1, 0, CapacityEdge::new(30, ConstRate(1)));

        cg.update_edge(1, 2, CapacityEdge::new(10, ConstRate(1)));
        cg.update_edge(2, 1, CapacityEdge::new(10, ConstRate(1)));

        cg.update_edge(2, 5, CapacityEdge::new(5, ConstRate(1)));
        cg.update_edge(5, 2, CapacityEdge::new(30, ConstRate(1)));

        cg.update_edge(1, 3, CapacityEdge::new(8, ConstRate(1)));
        cg.update_edge(3, 1, CapacityEdge::new(30, ConstRate(1)));

        cg.update_edge(3, 4, CapacityEdge::new(6, ConstRate(1)));
        cg.update_edge(4, 3, CapacityEdge::new(30, ConstRate(1)));

        cg.update_edge(4, 2, CapacityEdge::new(18, ConstRate(1)));
        cg.update_edge(2, 4, CapacityEdge::new(30, ConstRate(1)));

        cg
    }

    #[test]
    fn test_get_multi_route() {
        let cg = example_capacity_graph();

        let multi_route = cg.get_multi_route(&2, &5, 29, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        let multi_route = cg.get_multi_route(&2, &5, 30, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        assert!(cg.get_multi_route(&2, &5, 31, None).is_none());

        let multi_route = cg.get_multi_route(&0, &5, 25, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1, 3, 4, 2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        let multi_route = cg.get_multi_route(&0, &5, 29, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1, 3, 4, 2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        let multi_route = cg.get_multi_route(&0, &5, 30, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1, 3, 4, 2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        assert!(cg.get_multi_route(&0, &5, 31, None).is_none());

        // Block an essential edge:
        assert!(cg.get_multi_route(&0, &5, 25, Some((&3, &4))).is_none());

        // Block an essential edge but the at the reversed direction:
        let multi_route = cg.get_multi_route(&0, &5, 25, Some((&4, &3))).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1, 3, 4, 2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        // Block an edge not used for the route:
        let multi_route = cg.get_multi_route(&0, &5, 25, Some((&1, &2))).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1, 3, 4, 2, 5]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        // Use excluded edge to find a loop from 1 to 1:
        let multi_route = cg.get_multi_route(&2, &1, 6, Some((&2, &1))).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![2, 4, 3, 1]);
        assert_eq!(multi_route.routes[0].capacity, 6);

        // Request for too much capacity:
        assert!(cg.get_multi_route(&2, &1, 7, Some((&2, &1))).is_none());
    }

    #[test]
    fn test_simple_capacity_graph_tick() {
        let mut cg = SimpleCapacityGraph::<u32, ConstRate>::new();

        cg.update_edge(0, 1, CapacityEdge::new(30, ConstRate(1)));
        cg.update_edge(1, 0, CapacityEdge::new(30, ConstRate(1)));

        cg.update_edge(2, 3, CapacityEdge::new(10, ConstRate(1)));
        cg.update_edge(3, 2, CapacityEdge::new(30, ConstRate(1)));

        let multi_route = cg.get_multi_route(&0, &1, 30, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![0, 1]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        let multi_route = cg.get_multi_route(&2, &3, 30, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![2, 3]);
        assert_eq!(multi_route.routes[0].capacity, 30);

        let max_edge_age = max_edge_age(1);
        for _ in 0..max_edge_age - 1 {
            cg.tick(&0);

            let multi_route = cg.get_multi_route(&0, &1, 30, None).unwrap();
            assert_eq!(multi_route.routes[0].route, vec![0, 1]);
            assert_eq!(multi_route.routes[0].capacity, 30);

            let multi_route = cg.get_multi_route(&2, &3, 30, None).unwrap();
            assert_eq!(multi_route.routes[0].route, vec![2, 3]);
            assert_eq!(multi_route.routes[0].capacity, 30);
        }

        // At this point 0->1 and 1->0 should expire, but 2->3 and 3->2 don't expire:
        cg.tick(&0);
        assert!(cg.get_multi_route(&0, &1, 30, None).is_none());

        let multi_route = cg.get_multi_route(&2, &3, 30, None).unwrap();
        assert_eq!(multi_route.routes[0].route, vec![2, 3]);
        assert_eq!(multi_route.routes[0].capacity, 30);
    }
}
