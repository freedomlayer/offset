use std::{cmp, hash};
use std::collections::HashMap;

use crate::bfs::bfs;

/*
use proto::conn::BoxFuture;

trait Graph {
    type Request;
    type Mutation;

    fn request(&mut self, request: Self::Request) -> BoxFuture<'_, Self::Response>;
    fn mutate(&mut self, mutation: Self::Mutation) -> BoxFuture<'_, ()>;
}
*/

type CapacityEdge = (u128, u128);

struct CapacityGraph<N> {
    nodes: HashMap<N,HashMap<N,CapacityEdge>>,
}


impl<N> CapacityGraph<N> 
where
    N: cmp::Eq + hash::Hash + Clone,
{

    /// Add or update edge
    pub fn update_edge(&mut self, a: N, b: N, edge: CapacityEdge) -> Option<CapacityEdge> {
        let mut a_entry = self.nodes.entry(a).or_insert(HashMap::new());
        a_entry.insert(b, edge)
    }

    /// Remove an edge from the graph
    pub fn remove_edge(&mut self, a: &N, b: &N) -> Option<CapacityEdge> {
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
    pub fn remove_node(&mut self, a: &N) -> Option<HashMap<N, CapacityEdge>> {
        self.nodes.remove(a)
    } 

    /// Get a directed edge (if exists) 
    fn get_edge(&self, a: &N, b: &N) -> Option<CapacityEdge> {
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
        let (_b_send, b_recv) = a_b_edge;

        cmp::min(a_send, b_recv)
    }

    fn neighbors_with_send_capacity(&self, a: N, capacity: u128) -> Option<impl Iterator<Item=&N>> {
        let a_map = self.nodes.get(&a)?;
        let iter = a_map.keys().filter(move |b| self.get_send_capacity(&a,b) >= capacity);
        Some(iter)
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
    pub fn get_route(&self, a: &N, b: &N, capacity: u128) -> Option<(Vec<N>, u128)> {
        let get_neighbors = |b: &N| self.neighbors_with_send_capacity(b.clone(), capacity).unwrap();
        let route = bfs(a, b, get_neighbors)?;
        // We assert that we will always have valid capacity here:
        let capacity = self.get_route_capacity(&route).unwrap();

        Some((route, capacity))
    }

    /// A loop from myself through given friend, back to myself.
    /// self -> neighbor -> ... -> ... -> self
    pub fn get_loop_from(&self, a: &N, neighbor: &N, capacity: u128) -> Option<(Vec<N>, u128)> {
        let c_neighbor = neighbor.clone();
        let cloned_a = a.clone();
        let get_neighbors = move |cur_node: &N| {
            self.neighbors_with_send_capacity(cur_node.clone(), capacity)
                .unwrap()
                .filter(|&next_node| (cur_node != &c_neighbor) || (next_node != &cloned_a))
        };

        let route = bfs(a, neighbor, get_neighbors)?;
        // We assert that we will always have valid capacity here:
        let capacity = self.get_route_capacity(&route).unwrap();

        Some((route, capacity))
    }

    pub fn get_loop_to(&self, a: &N, neighbor: &N, capacity: u128) {
        unimplemented!();
    }
}

