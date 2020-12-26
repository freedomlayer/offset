use std::collections::HashSet;
use std::hash::Hash;

use proto::consts::MAX_ROUTE_LEN;

pub trait Route<T> {
    /// Check if the route (e.g. `FriendsRoute`) is valid.
    /// A valid route must have at least 2 unique nodes, and is in one of the following forms:
    /// A -- B -- C -- D -- E -- F -- A   (Single cycle, first == last)
    /// A -- B -- C -- D -- E -- F        (A route with no repetitions)
    fn is_valid(&self) -> bool;

    /// Checks if the remaining part of the route (e.g. `FriendsRoute`) is valid.
    /// Compared to regular version, this one does not check for minimal unique
    /// nodes amount. It returns `true` if the part is empty.
    /// It does not accept routes parts with a cycle, though.
    fn is_part_valid(&self) -> bool;
}

/// Check if no element repeats twice in the slice
fn no_duplicates<T: Hash + Eq>(array: &[T]) -> bool {
    let mut seen = HashSet::new();
    for item in array {
        if !seen.insert(item) {
            return false;
        }
    }
    true
}

fn is_route_valid<T>(route: &[T]) -> bool
where
    T: Hash + Eq,
{
    if route.len() < 2 {
        return false;
    }
    if route.len() > MAX_ROUTE_LEN {
        return false;
    }

    // route.len() >= 2
    let last_key = route.last().unwrap();
    if last_key == &route[0] {
        // We have a first == last cycle.
        if route.len() > 2 {
            // We have a cycle that is long enough (no A -- A).
            // We just check if it's a single cycle.
            no_duplicates(&route[1..])
        } else {
            // A -- A
            false
        }
    } else {
        // No first == last cycle.
        // But we have to check if there is any other cycle.
        no_duplicates(&route)
    }
}

fn is_route_part_valid<T>(route: &[T]) -> bool
where
    T: Hash + Eq,
{
    // Route part should not be full route.
    // TODO: ensure it never is.
    if route.len() >= MAX_ROUTE_LEN {
        return false;
    }

    no_duplicates(route)
}

impl<T, R> Route<T> for R
where
    T: Hash + Eq,
    R: AsRef<[T]>,
{
    fn is_valid(&self) -> bool {
        is_route_valid(self.as_ref())
    }

    fn is_part_valid(&self) -> bool {
        is_route_part_valid(self.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::Route;

    #[test]
    fn test_friends_is_route_valid() {
        assert_eq!([1].is_valid(), false); // too short
        assert_eq!([1].is_part_valid(), true); // long enough
        assert_eq!(Vec::<u8>::new().is_valid(), false); // empty route is invalid
        assert_eq!(Vec::<u8>::new().is_part_valid(), true); // partial routes may be empty

        // Test cases taken from https://github.com/freedomlayer/offset/pull/215#discussion_r292327613
        assert_eq!([1, 2, 3, 4].is_valid(), true); // usual route
        assert_eq!([1, 2, 3, 4, 1].is_valid(), true); // cyclic route that is at least 3 nodes long, having first item equal the last item
        assert_eq!([1, 1].is_valid(), false); // cyclic route that is too short (only 2 nodes long)
        assert_eq!([1, 2, 3, 2, 4].is_valid(), false); // Should have no repetitions that are not the first and last nodes.

        assert_eq!([1, 2, 3, 4].is_part_valid(), true); // usual route
        assert_eq!([1, 2, 3, 4, 1].is_part_valid(), false); // should have no cycles in a partial route
        assert_eq!([1, 1].is_part_valid(), false); // should have no repetitions ins a partial route
        assert_eq!([1, 2, 3, 2, 4].is_part_valid(), false); // should have no repetitions in a partial route

        assert_eq!(vec![1, 2, 3, 2, 4].is_part_valid(), false); // should have no repetitions in a partial route
        assert_eq!((&[1, 2, 3, 2, 4]).is_part_valid(), false); // should have no repetitions in a partial route
    }
}
