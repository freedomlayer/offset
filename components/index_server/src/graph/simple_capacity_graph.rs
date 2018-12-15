use std::{cmp, hash};
use std::collections::HashMap;

use super::bfs::bfs;
use super::capacity_graph::{CapacityGraph, CapacityEdge};
use super::utils::{option_to_vec, OptionIterator};


pub struct SimpleCapacityGraph<N> {
    nodes: HashMap<N,HashMap<N,CapacityEdge<u128>>>,
}


impl<N> SimpleCapacityGraph<N> 
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
{

    pub fn new() -> SimpleCapacityGraph<N> {
        SimpleCapacityGraph {
            nodes: HashMap::new(),
        }
    }

    /// Get a directed edge (if exists) 
    fn get_edge(&self, a: &N, b: &N) -> Option<CapacityEdge<u128>> {
        match self.nodes.get(a) {
            None => None,
            Some(a_map) => {
                match a_map.get(b) {
                    None => None,
                    Some(a_b_edge) => Some(*a_b_edge),
                }
            }
        }
    }

    /// Get the send capacity from `a` to a direct neighbor `b`.
    /// This is calculated as the minimum send capacity reported by `a` and the maximum recv
    /// capacity reported by `b`.
    fn get_send_capacity(&self, a: &N, b: &N) -> u128 {
        let a_b_edge = if let Some(a_b_edge) = self.get_edge(&a,&b) {
            a_b_edge 
        } else {
            return 0;
        };

        let b_a_edge = if let Some(b_a_edge) = self.get_edge(&b,&a) {
            b_a_edge 
        } else {
            return 0;
        };

        let (a_send, _a_recv) = a_b_edge;
        let (_b_send, b_recv) = b_a_edge;

        cmp::min(a_send, b_recv)
    }

    fn neighbors_with_send_capacity(&self, a: N, capacity: u128) -> OptionIterator<impl Iterator<Item=&N>> {
        let a_map = match self.nodes.get(&a) {
            Some(a_map) => a_map,
            None => return OptionIterator::new(None),
        };
        let iter = a_map.keys().filter(move |b| self.get_send_capacity(&a,b) >= capacity);
        OptionIterator::new(Some(iter))
    }

    /// Calculate the amount of capacity we can send through a route.
    /// This amount if the minimum of all edge capacities of the route.
    fn get_route_capacity(&self, route: &[N]) -> Option<u128> {
        (0 .. route.len().checked_sub(1)?)
            .map(|i| self.get_send_capacity(&route[i], &route[i+1]))
            .min()
    }

    /// Get a route with capacity at least `capacity`. 
    /// Returns the route together with the capacity it is possible to send through the route.
    ///
    /// opt_exclude is an optional edge to exclude (The returned route must not go through this
    /// edge). This can be useful for finding non trivial loops.
    fn get_route(&self, a: &N, b: &N, capacity: u128, opt_exclude: Option<(&N, &N)>) -> Option<(Vec<N>, u128)> {
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

        Some((route, capacity))
    }

}

impl<N> CapacityGraph for SimpleCapacityGraph<N> 
where
    N: cmp::Eq + hash::Hash + Clone + std::fmt::Debug,
{
    type Node = N;
    type Capacity = u128;


    /// Add or update edge
    fn update_edge(&mut self, a: N, b: N, edge: CapacityEdge<u128>) -> Option<CapacityEdge<u128>> {
        let mut a_entry = self.nodes.entry(a).or_insert(HashMap::new());
        a_entry.insert(b, edge)
    }

    /// Remove an edge from the graph
    fn remove_edge(&mut self, a: &N, b: &N) -> Option<CapacityEdge<u128>> {
        let mut a_map = match self.nodes.get_mut(a) {
            Some(a_map) => a_map,
            None => return None,
        };

        let old_edge = match a_map.remove(b) {
            Some(edge) => edge,
            None => return None,
        };

        if a_map.len() == 0 {
            self.nodes.remove(a);
        }

        Some(old_edge)
    }

    /// Remove a node and all related edges known from him.
    /// Note: This method will not remove an edge from another node b pointing to a.
    fn remove_node(&mut self, a: &N) -> bool {
        match self.nodes.remove(a) {
            Some(_) => true,
            None => false,
        }
    } 

    fn get_routes(&self, a: &N, b: &N, capacity: u128, opt_exclude: Option<(&N, &N)>) -> Vec<(Vec<N>, u128)> {
        option_to_vec(self.get_route(a, b, capacity, opt_exclude))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_send_capacity_basic() {
        let mut cg = SimpleCapacityGraph::<u32>::new();
        cg.update_edge(0, 1, (10, 20));
        cg.update_edge(1, 0, (15, 5));

        assert_eq!(cg.get_send_capacity(&0, &1), cmp::min(5, 10));
        assert_eq!(cg.get_send_capacity(&1, &0), cmp::min(15, 20));
    }

    #[test]
    fn test_get_send_capacity_one_sided() {
        let mut cg = SimpleCapacityGraph::<u32>::new();
        cg.update_edge(0, 1, (10, 20));

        assert_eq!(cg.get_send_capacity(&0, &1), 0);
        assert_eq!(cg.get_send_capacity(&1, &0), 0);
    }

    #[test]
    fn test_add_remove_edge() {
        let mut cg = SimpleCapacityGraph::<u32>::new();
        assert_eq!(cg.remove_edge(&0, &1), None);
        cg.update_edge(0, 1, (10, 20));
        assert_eq!(cg.nodes.len(), 1);

        assert_eq!(cg.remove_edge(&0, &1), Some((10,20)));
        assert_eq!(cg.nodes.len(), 0);

        cg.update_edge(0, 1, (10,20));
        assert_eq!(cg.nodes.len(), 1);
        cg.remove_node(&1);
        assert_eq!(cg.nodes.len(), 1);
    }

    fn example_capacity_graph() -> SimpleCapacityGraph<u32> {
        /*
         * Example graph:
         *
         *    0 --> 1 --> 2 --> 5
         *          |     ^
         *          V     |
         *          3 --> 4
         *
        */

        let mut cg = SimpleCapacityGraph::<u32>::new();

        cg.update_edge(0, 1, (30, 10));
        cg.update_edge(1, 0, (10, 30));

        cg.update_edge(1, 2, (10, 10));
        cg.update_edge(2, 1, (10, 10));

        cg.update_edge(2, 5, (30, 5));
        cg.update_edge(5, 2, (5, 30));

        cg.update_edge(1, 3, (30, 8));
        cg.update_edge(3, 1, (8, 30));

        cg.update_edge(3, 4, (30, 6));
        cg.update_edge(4, 3, (6, 30));

        cg.update_edge(4, 2, (30, 18));
        cg.update_edge(2, 4, (18, 30));

        cg
    }

    #[test]
    fn test_get_route() {
        let mut cg = example_capacity_graph();

        assert_eq!(cg.get_route(&2, &5, 29, None), Some((vec![2,5], 30)));
        assert_eq!(cg.get_route(&2, &5, 30, None), Some((vec![2,5], 30)));
        assert_eq!(cg.get_route(&2, &5, 31, None), None);

        assert_eq!(cg.get_route(&0, &5, 25, None), Some((vec![0,1,3,4,2,5], 30)));
        assert_eq!(cg.get_route(&0, &5, 29, None), Some((vec![0,1,3,4,2,5], 30)));
        assert_eq!(cg.get_route(&0, &5, 30, None), Some((vec![0,1,3,4,2,5], 30)));
        assert_eq!(cg.get_route(&0, &5, 31, None), None);

        // Block an essential edge:
        assert_eq!(cg.get_route(&0, &5, 25, Some((&3,&4))), None);
        // Block an essential edge but the at the reversed direction:
        assert_eq!(cg.get_route(&0, &5, 25, Some((&4,&3))), Some((vec![0,1,3,4,2,5], 30)));
        // Block an edge not used for the route:
        assert_eq!(cg.get_route(&0, &5, 25, Some((&1,&2))), Some((vec![0,1,3,4,2,5], 30)));

        // Use excluded edge to find a loop from 1 to 1:
        assert_eq!(cg.get_route(&2, &1, 6, Some((&2,&1))), Some((vec![2,4,3,1], 6)));
        // Require too much capacity:
        assert_eq!(cg.get_route(&2, &1, 7, Some((&2,&1))), None);
    }
}

