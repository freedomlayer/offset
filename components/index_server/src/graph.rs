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

    pub fn get_route(&self, a: &N, b: &N, capacity: u128) -> Option<Vec<N>> {
        let get_neighbors = |b: &N| self.neighbors_with_send_capacity(b.clone(), capacity).unwrap();
        bfs(a, b, get_neighbors)
    }

    pub fn get_loop_from(&self, a: N, b: N, capacity: u128) {
        unimplemented!();
    }

    pub fn get_loop_to(&self, a: N, b: N, capacity: u128) {
        unimplemented!();
    }
}

