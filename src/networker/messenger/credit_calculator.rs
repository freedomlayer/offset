use std::cmp;
use crypto::identity::PublicKey;
use proto::indexer::NeighborsRoute;


/// Calculate the amount of credits that needs to be frozen
/// for the destination (By the node before the destination).
pub fn calc_dest_freeze_credits(route: &NeighborsRoute,
                            processing_fee_proposal: u64,
                            max_response_len: u32) -> Option<u64> {

    // TODO: Make this function more generic, to deal with routes of length 2?
    // Currently this function assumes that rl.len() > 0.
    /*
    processing_fee 
        + {FE}_b + max_response_len * {FE}_r +
        + (max_response_len - response_len) * ({CB}_r + {DC}_r + {ED}_r) 

    We set response_len = 0 to obtain the maximum amount of required credit.
    */

    // Find out how many credits we need to freeze:
    let rl = &route.route_links;

    // Sum of multiplier of responses, not including the response mutliplier of the
    // destionation
    let mut sum_resp_multiplier: u64 = 0;
    for route_link in &rl[0 .. rl.len() - 1] {
        sum_resp_multiplier = sum_resp_multiplier.checked_add(
            u64::from(route_link.response_payment_proposal.0.multiplier))?;
    }

    let resp_prop = &rl[rl.len() - 1].response_payment_proposal;

    let credits_freeze_dest = 
            processing_fee_proposal
            .checked_add(u64::from(resp_prop.0.base))?
            .checked_add(
                u64::from(resp_prop.0.multiplier).checked_mul(u64::from(max_response_len))?)?
            .checked_add(
                u64::from(max_response_len).checked_mul(sum_resp_multiplier)?)?;

    Some(credits_freeze_dest)
}


// CR(a4vision): Let's discuss it - maybe it is better to introduce explicit formulas,
//                  by converting to i128 - to make the code more readable.

/// Amount of credits paid to destination node, upon issuing a signed Response message.
/// The destination node is the last node along the route of a request.
/// Upon any overflow (u64) this function will return None.
fn credits_on_success_dest(processing_fee_proposal: u64, request_len: u32, credits_per_byte_proposal: u64,
                           response_len: u32, max_response_len: u32) -> Option<u64> {

    // processing_fee_proposal + request_len * creditsPerBytesProposal + (max_response_len - response_len)
    processing_fee_proposal.checked_add(
    u64::from(request_len).checked_mul(credits_per_byte_proposal)?
    )?.checked_add(
        u64::from(max_response_len).checked_sub(u64::from(response_len))?
    )
}


/// Amount of credit paid to a node that sent a valid Response (Which closes an open request).
/// This amount depends upon the original request, and also on the position of the node along the
/// route used for sending the request (This is represented by the `nodes_to_dest` argument).
/// `credits_on_success_dest` is a special case of this function, for `nodes_to_dest` = 1.
///
/// Example:
/// A -- B -- (C) -- D -- E
///
/// The edge (C,D) has `node_to_dest` = 2, the edge (B,C) has `nodes_to_dest`=3.
/// This function calculates the amount of credits C should obtain if he sent back a signed
/// Response message to B, as a response to a Request sent from A all the way to E.
/// Upon any overflow (u64) this function will return None.
pub fn credits_on_success(processing_fee_proposal: u64, request_len: u32, 
                     credits_per_byte_proposal: u64, max_response_len: u32, 
                     response_len: u32, nodes_to_dest: usize) -> Option<u64> {

    // (request_len + response_len) * credits_per_byte_proposal * (nodes_to_dest - 1) +
    //      credits_on_success_dest(...)
    (u64::from(request_len)).checked_add(u64::from(response_len))?
        .checked_mul(credits_per_byte_proposal)?
        .checked_mul((nodes_to_dest.checked_sub(1)?) as u64)?
        .checked_add(
            credits_on_success_dest(processing_fee_proposal, 
                                    request_len, 
                                    credits_per_byte_proposal,
                                    response_len,
                                    max_response_len)?)
}

/// The amount of credits paid to a node in case of failure.
/// This amount depends on the length of the Request message, 
/// and also on the amount of nodes until the reporting node.
/// Example:
///
/// A -- B -- C -- D -- E
///
/// Asssume that A sends a Request message along the route in the picture all the way to E.
/// Assume that D is not willing to pass the message to E for some reason, and therefore he reports
/// a failure message back to C. In this case, for example:
///     - D will receive `credits_on_failure(request_len, 1)` credits.
///     - C will receive `credits_on_failure(request_len, 2)` credits.
/// In other words,
///     - The amount of credit transferred on the edge (C, D) is `credits_on_failure(request_len, 1)`
///     - The amount of credit transferred on the edge (B, C) is `credits_on_failure(request_len, 2)`
pub fn credits_on_failure(request_len: u32, nodes_to_reporting: usize) -> Option<u64> {
    // request_len * nodes_to_reporting
    u64::from(request_len).checked_mul(nodes_to_reporting as u64)
}

/// Compute the amount of credits we need to freeze on an edge along a request route.
/// Example:
///
/// A -- B -- (C) -- D -- E
///
/// The node A sends a request along the route in the picture, all the way to E.  Here we can
/// compute for example the amount of credits to freeze between B and C.
/// This amount is `credits_on_success_dest(..., nodes_to_dest=3)`
///
/// Note that when C RECEIVES the request is should freeze
///     `credits_on_success_dest(..., nodes_to_dest=3)`
/// but when it SENDS the request, it should freeze
///     `credits_on_success_dest(..., nodes_to_dest=2)`
pub fn credits_to_freeze(processing_fee_proposal: u64, request_len: u32,
                         credits_per_byte_proposal: u64, max_response_len: u32,
                         nodes_to_dest: usize) -> Option<u64> {

    // Note: Here we take the maximum for credits_on_success for the cases of:  
    // - resposne_len = 0
    // - response_len = max_response_len.  
    // We do this because credits_on_success is linear with respect to the response_len argument,
    // hence the maximum of credits_on_success must be on one of the edges.
    
    let credits_resp_len_zero = credits_on_success(processing_fee_proposal, 
                       request_len,
                       credits_per_byte_proposal, 
                       max_response_len,
                       0,               // Minimal response_len
                       nodes_to_dest)?;
    let credits_resp_len_max = credits_on_success(processing_fee_proposal, 
                       request_len,
                       credits_per_byte_proposal, 
                       max_response_len,
                       max_response_len, // Maximum response len
                       nodes_to_dest)?;

    Some(cmp::max(credits_resp_len_zero, credits_resp_len_max))
}


#[cfg(test)]
mod tests {
    use super::*;

    // CR(a4vision): Re-think about these tests, it reminds "configuration-testing"
    //              I think that edge cases (overflow) MUST be checked.
    #[test]
    fn tests_credits_on_success_dest_basic() {
        let processing_fee_proposal = 5;
        let request_len = 10;
        let credits_per_byte_proposal = 2;
        let response_len = 5;
        let max_response_len = 10;

        let num_credits = credits_on_success_dest(
            processing_fee_proposal,
            request_len,
            credits_per_byte_proposal,
            response_len,
            max_response_len).unwrap();

        assert_eq!(num_credits, 5 + 10 * 2 + (10 - 5));
    }

    #[test]
    fn test_credits_on_success_basic() {
        // TODO
    }

    #[test]
    fn test_credits_on_failure_basic() {
        // TODO
    }

    #[test]
    fn credits_to_freeze_basic() {
        // TODO
    }
}

