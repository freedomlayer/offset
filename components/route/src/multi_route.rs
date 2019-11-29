use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;
use num_traits::ops::checked::CheckedSub;

use proto::index_server::messages::MultiRoute;

/// For every route in a multi route: How many credits to push through.
pub type MultiRouteChoice = Vec<(usize, u128)>; // (route_index, credits_to_push)

/// Attempt to fill the given amount as much as possible, trying not to saturate any route.
/// Returns `Some((i, added_credits(i)))` if in index `i` we can already fill `amount_left`, or
/// `None` if we can never fill the wanted amount.
fn fill_amount(amount: u128, sorted_routes: &[(Option<usize>, u128)]) -> Option<(usize, BigUint)> {
    let mut amount_left = BigUint::from(amount);
    // Chosen route indices (Indices with respect to the original `routes`)
    for i in 1..sorted_routes.len() {
        let prev_i = i.checked_sub(1).unwrap();
        // sorted_routes is sorted, therefore we can be sure that the next
        // checked_sub(...).unwrap() will not panic:
        let added_credits: BigUint = BigUint::from(
            sorted_routes[prev_i]
                .1
                .checked_sub(sorted_routes[i].1)
                .unwrap(),
        ) * i;

        if let Some(new_amount_left) = amount_left.checked_sub(&added_credits) {
            amount_left = new_amount_left;
        } else {
            return Some((i, amount_left));
        }
    }
    None
}

/// Find a safe choice for how much credits to push through each route in a MultiRoute.
/// Returns a vector representing how many credits to push through every chosen route (if successful).
/// For example: (5usize, 100u128) means: push 100 credits through the route that was given in
/// index 5.
fn safe_multi_route_amounts(multi_route: &MultiRoute, amount: u128) -> Option<MultiRouteChoice> {
    let routes = &multi_route.routes;
    let sorted_routes = {
        let mut sorted_routes: Vec<_> = routes
            .iter()
            .enumerate()
            .map(|(j, route)| (Some(j), route.rate.max_payable(route.capacity)))
            .collect();

        // Reverse sort: (Largest is first)
        sorted_routes.sort_by(|(_, a), (_, b)| b.cmp(a));
        // Add a zero entry in the end:
        sorted_routes.push((None, 0u128));
        sorted_routes
    };

    let (num_routes, amount_left) = fill_amount(amount, &sorted_routes[..])?;
    // Add this amount to the first `num_routes` biggest entries:
    let div_extra = (&amount_left / num_routes).to_u128().unwrap();
    // Add `1` to this amount of elements:
    let mod_extra = (&amount_left % num_routes).to_u128().unwrap();

    let mut chosen_routes = Vec::new();
    let mut accum_credits = 0u128;
    for i in (0..num_routes).rev() {
        // Add the extra part to accum_credits
        let num_credits = accum_credits.checked_add(div_extra).unwrap();
        let num_credits = if (i as u128) < mod_extra {
            num_credits.checked_add(1).unwrap()
        } else {
            num_credits
        };
        chosen_routes.push((sorted_routes[i].0.unwrap(), num_credits));

        // TODO: How to do this part more elegantly?
        if let Some(prev_i) = i.checked_sub(1) {
            let diff = sorted_routes[prev_i]
                .1
                .checked_sub(sorted_routes[i].1)
                .unwrap();
            accum_credits = accum_credits.checked_add(diff).unwrap();
        }
    }
    Some(chosen_routes)
}

/// Choose a route for pushing `amount` credits
pub fn choose_multi_route(
    multi_routes: &[MultiRoute],
    amount: u128,
) -> Option<(usize, MultiRouteChoice)> {
    // We naively select the first multi-route we find suitable:
    // TODO: Possibly improve this later:
    for (i, multi_route) in multi_routes.iter().enumerate() {
        if let Some(multi_route_choice) = safe_multi_route_amounts(multi_route, amount) {
            return Some((i, multi_route_choice));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use proto::crypto::PublicKey;
    use proto::funder::messages::{FriendsRoute, Rate};
    use proto::index_server::messages::RouteCapacityRate;

    /// A helper function to create a test public key
    fn pk(i: u8) -> PublicKey {
        PublicKey::from(&[i; PublicKey::len()])
    }

    #[test]
    fn test_safe_multi_route_amounts() {
        let mut multi_route = MultiRoute { routes: vec![] };
        multi_route.routes.push(RouteCapacityRate {
            route: FriendsRoute {
                public_keys: vec![pk(0), pk(1), pk(2), pk(3), pk(4)],
            },
            capacity: 100u128,
            rate: Rate {
                add: 1,
                mul: 0x12345678,
            },
        });

        multi_route.routes.push(RouteCapacityRate {
            route: FriendsRoute {
                public_keys: vec![pk(0), pk(5), pk(6), pk(4)],
            },
            capacity: 200u128,
            rate: Rate {
                add: 5,
                mul: 0x00100000,
            },
        });

        multi_route.routes.push(RouteCapacityRate {
            route: FriendsRoute {
                public_keys: vec![pk(0), pk(7), pk(8), pk(9), pk(4)],
            },
            capacity: 300u128,
            rate: Rate {
                add: 20,
                mul: 0x20000000,
            },
        });
        assert!(safe_multi_route_amounts(&multi_route, 601).is_none());

        let multi_route_choice = safe_multi_route_amounts(&multi_route, 300).unwrap();

        let mut total_credits = 0u128;
        for route_choice in &multi_route_choice {
            let route = &multi_route.routes[route_choice.0];
            // Make sure that we don't take too much:
            assert!(route_choice.1 < route.rate.max_payable(route.capacity));
            total_credits = total_credits.checked_add(route_choice.1).unwrap();
        }
        assert_eq!(total_credits, 300);
    }
}
