use num_bigint::BigUint;
// use num_traits::identities::Zero;
use num_traits::cast::ToPrimitive;
use num_traits::ops::checked::CheckedSub;

use app::route::MultiRoute;

/*
/// Can we push the given amount of credits through this multi route?
fn is_good_multi_route(multi_route: &MultiRoute, mut amount: u128) -> bool {
    let mut credit_count = 0u128;

    for route_capacity_rate in &multi_route.routes {
        let max_payable = route_capacity_rate
            .rate
            .max_payable(route_capacity_rate.capacity);
        credit_count = if let Some(new_credit_count) = credit_count.checked_add(max_payable) {
            new_credit_count
        } else {
            // An overflow happened. This means we can definitely pay `amount`.
            return true;
        };
    }

    credit_count >= amount
}
*/

pub type MultiRouteChoice = Vec<(usize, u128)>;

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
            sorted_routes[i]
                .1
                .checked_sub(sorted_routes[prev_i].1)
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
        let next_i = i.checked_add(1).unwrap();
        let diff = sorted_routes[next_i]
            .1
            .checked_sub(sorted_routes[i].1)
            .unwrap();
        accum_credits = accum_credits.checked_add(diff).unwrap();

        let num_credits = accum_credits.checked_add(div_extra).unwrap();
        let num_credits = if (i as u128) < mod_extra {
            num_credits.checked_add(1).unwrap()
        } else {
            num_credits
        };
        chosen_routes.push((sorted_routes[i].0.unwrap(), num_credits));
    }
    Some(chosen_routes)
}

#[allow(unused)]
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

/*
#[cfg(test)]
mod tests {
    use super::*;
}
*/
