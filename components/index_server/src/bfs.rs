use std::{cmp, hash};
use std::collections::{HashMap, HashSet, VecDeque};

fn bfs_loop<'c,I,N,F>(src: &'c N, dst: &'c N, get_neighbors: F) -> Option<HashMap<N,Option<N>>>
where
    I: Iterator<Item=&'c N>,
    F: Fn(&N) -> I,
    N: Clone + cmp::Eq + hash::Hash,
{
    let mut backtrack: HashMap<N,Option<N>> = HashMap::new();
    let mut visited: HashSet<N> = HashSet::new();
    let mut queue: VecDeque<N> = VecDeque::new();

    backtrack.insert(src.clone(), None);
    queue.push_back(src.clone());
    visited.insert(src.clone());

    while let Some(node) = queue.pop_front() {
        for neighbor in get_neighbors(&node) {
            if visited.contains(&neighbor) {
                continue;
            }
            backtrack.insert(neighbor.clone(), Some(node.clone()));
            if neighbor == dst {
                return Some(backtrack);
            }
            queue.push_back(neighbor.clone());
            visited.insert(neighbor.clone());
        }
    }
    None
}

fn bfs_backtrack<N>(dst: &N, backtrack: &HashMap<N, Option<N>>) -> Option<Vec<N>> 
where
    N: Clone + cmp::Eq + hash::Hash,
{
    let mut route = Vec::new();

    route.push(dst.clone());
    let mut node = dst;

    while let Some(new_node) = backtrack.get(node)? {
        route.push(node.clone());
        node = new_node;
    }

    route.reverse();
    Some(route)
}


pub fn bfs<'c,I,N,F>(src: &'c N, dst: &'c N, get_neighbors: F) -> Option<Vec<N>>
where
    I: Iterator<Item=&'c N>,
    F: Fn(&N) -> I,
    N: Clone + cmp::Eq + hash::Hash,
{
    let backtrack = bfs_loop(src, dst, get_neighbors)?;
    bfs_backtrack(dst, &backtrack)
}
