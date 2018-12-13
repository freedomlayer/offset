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
        route.push(new_node.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bfs_backtrack_basic() {
        let mut backtrack: HashMap<u32, Option<u32>> = HashMap::new();
        /*
         *    3 -- 4
         *     \-- 5 -- 7
         *     \-- 6 -- 8 -- 9 -- 10
         *                   \-- 11
         *
        */
        backtrack.insert(3, None);
        backtrack.insert(4, Some(3));
        backtrack.insert(5, Some(3));
        backtrack.insert(6, Some(3));
        backtrack.insert(7, Some(5));
        backtrack.insert(8, Some(6));
        backtrack.insert(9, Some(8));
        backtrack.insert(10, Some(9));
        backtrack.insert(11, Some(9));

        let opt_route = bfs_backtrack(&11, &backtrack);
        assert_eq!(opt_route.unwrap(), vec![3,6,8,9,11]);
    }

    #[test]
    fn test_bfs_backtrack_failure() {
        let mut backtrack: HashMap<u32, Option<u32>> = HashMap::new();
        backtrack.insert(2, Some(1));
        backtrack.insert(3, Some(2));

        // Backtracking should fail, because 1 is not a key at the backtrack map:
        assert!(bfs_backtrack(&3, &backtrack).is_none());
    }

    #[test]
    fn test_bfs_basic() {
        /*
         Example graph:
                            0 --> 1
                            ^     |
                            |     |
               9            |     V
               ^            3 <-- 2
               |            |
               |            V
               8 <-- 6 <--- 4 --> 5
                     ^
                     |
                     V
                     7
        */

        let mut graph = HashMap::new();
        graph.insert(0u32, vec![1u32]);
        graph.insert(1, vec![2]);
        graph.insert(2, vec![3]);
        graph.insert(3, vec![0,4]);
        graph.insert(4, vec![5,6]);
        graph.insert(5, vec![]);
        graph.insert(6, vec![7,8]);
        graph.insert(7, vec![6]);
        graph.insert(8, vec![9]);
        graph.insert(9, vec![]);

        let get_neighbors = |node: &u32| graph.get(&node).unwrap().iter();
        assert_eq!(bfs(&0,&1,get_neighbors), Some(vec![0,1]));
        assert_eq!(bfs(&1,&0,get_neighbors), Some(vec![1,2,3,0]));

        assert_eq!(bfs(&0,&9,get_neighbors), Some(vec![0,1,2,3,4,6,8,9]));

        assert_eq!(bfs(&8,&6,get_neighbors), None);
        assert_eq!(bfs(&9,&8,get_neighbors), None);
        assert_eq!(bfs(&5,&4,get_neighbors), None);
        assert_eq!(bfs(&4,&3,get_neighbors), None);

        assert_eq!(bfs(&6,&7,get_neighbors), Some(vec![6,7]));
        assert_eq!(bfs(&7,&6,get_neighbors), Some(vec![7,6]));
    }
}
