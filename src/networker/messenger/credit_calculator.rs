use std::convert::TryFrom;
use std::{mem, cmp};
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use proto::indexer::PaymentProposalPair;
use proto::networker::NetworkerSendPrice;
use utils::int_convert::usize_to_u32;
use super::messenger_messages::NeighborFreezeLink;

pub struct PaymentProposals {
    middle_props: Vec<PaymentProposalPair>,
    dest_response_pay_props: NetworkerSendPrice,
}


fn calc_request_len(request_content_len: u32, 
                    route_len: u32, 
                    nodes_to_dest: u32) -> Option<u32> {

    let public_key_len = usize_to_u32(mem::size_of::<PublicKey>())?;
    let payment_proposal_pair_len = usize_to_u32(mem::size_of::<PaymentProposalPair>())?;
    let networker_send_price_len = usize_to_u32(mem::size_of::<NetworkerSendPrice>())?;
    let neighbor_freeze_link_len = usize_to_u32(mem::size_of::<NetworkerSendPrice>())?;

    let route_bytes_count = public_key_len.checked_mul(2)?
        .checked_add(route_len.checked_mul(public_key_len)?)?
        .checked_add(payment_proposal_pair_len)?
        .checked_add(networker_send_price_len)?;

    let freeze_links_len = route_len.checked_sub(nodes_to_dest)?
        .checked_mul(neighbor_freeze_link_len)?;

    let request_overhead = usize_to_u32(mem::size_of::<Uid>())?
        .checked_add(usize_to_u32(mem::size_of::<u32>())?)?
        .checked_add(usize_to_u32(mem::size_of::<u64>())?)?;

    Some(request_overhead.checked_add(route_bytes_count)?.checked_add(freeze_links_len)?)
}

fn calc_response_len(response_content_len: u32) -> Option<u32> {
    Some(usize_to_u32(mem::size_of::<Uid>())?
        .checked_add(usize_to_u32(mem::size_of::<RandValue>())?)?
        .checked_add(usize_to_u32(mem::size_of::<u64>())?)?
        .checked_add(response_content_len)?
        .checked_add(usize_to_u32(mem::size_of::<Signature>())?)?)
}

fn calc_failure_len(nodes_to_reporting_node: u32) -> Option<u32> {
    unreachable!(); // TODO
}

/// Amount of credits paid to destination node, upon issuing a signed Response message.
/// The destination node is the last node along the route of a request.
/// Upon any overflow (u64) this function will return None.
///           req      req      req
///           res      res      res      res
///    B  --   C   --   D   --   E   --   F   
///
/// credits_on_success_dest = 
///     processing_fee 
///     + {FE}_b + max_response_len * {FE}_r +
///     + (max_response_len - response_len) * ({CB}_r + {DC}_r + {ED}_r) 
///
pub fn credits_on_success_dest(payment_proposals: &PaymentProposals,
                               processing_fee_proposal: u64,
                               response_content_len: u32,
                               max_response_content_len: u32) -> Option<u64> {

    let middle_props_len = {
        if payment_proposals.middle_props.len() > u32::max_value() as usize {
            return None;
        } else {
            payment_proposals.middle_props.len() as u32
        }
    };

    let response_len = calc_response_len(response_content_len)?;
    let max_response_len = calc_response_len(max_response_content_len)?;

    // Find out how many credits we need to freeze:
    let mut sum_resp_multiplier: u64 = 0;
    for middle_prop in &payment_proposals.middle_props {
        sum_resp_multiplier = sum_resp_multiplier.checked_add(
            u64::from(middle_prop.response.0.multiplier))?;
    }

    let resp_prop = &payment_proposals.dest_response_pay_props;
    let credits_freeze_dest = 
            processing_fee_proposal
            .checked_add(u64::from(resp_prop.0.base))?
            .checked_add(
                u64::from(resp_prop.0.multiplier).checked_mul(
                    u64::from(max_response_len).checked_sub(u64::from(response_len))?
                )?
            )?
            .checked_add(
                u64::from(max_response_len).checked_mul(sum_resp_multiplier)?)?;

    Some(credits_freeze_dest)
}


/// Amount of credit paid to a node that sent a valid Response (Which closes an open request).
///           req      req      req
///           res      res      res      res
///    B  --  (C)  --   D   --   E   --   F   
///
/// Examples: C has 3 nodes to dest.
/// D has 2 nodes to dest. E has 1 nodes to dest. F has 0 nodes to dest.
///
/// Amount of credits C should earn for a successful delivery of the message:
///
/// {CD}_b + request_len * {CD}_r
///    + {CB}_b + (response_len + max_failure_len) * {CB}_r 
///
/// Upon any overflow (u64) this function will return None.
///
pub fn credits_on_success(payment_proposals: &PaymentProposals,
                          processing_fee_proposal: u64,
                          request_content_len: u32,
                          response_content_len: u32,
                          max_response_content_len: u32,
                          nodes_to_dest: u32) -> Option<u64> {

    let middle_props = &payment_proposals.middle_props;
    let middle_props_len = {
        if middle_props.len() > u32::max_value() as usize {
            return None;
        } else {
            middle_props.len() as u32
        }
    };

    if nodes_to_dest >= middle_props_len {
        return None;
    }

    let mut sum_credits: u64 = credits_on_success_dest(payment_proposals,
                                                       processing_fee_proposal,
                                                       response_content_len,
                                                       max_response_content_len)?;

    for i in (middle_props_len - nodes_to_dest - 1 .. middle_props_len).rev() {
        let middle_prop = &middle_props[i as usize];

        // TODO; Check for off by one here:
        let request_len = calc_request_len(request_content_len,
                                           middle_props_len,
                                           middle_props_len - i)?;
        let response_len = calc_request_len(response_content_len,
                                           middle_props_len,
                                           middle_props_len - i)?;
        // Maximum failure length occurs when the reporting node is as far as possible.
        let max_failure_len = calc_failure_len(middle_props_len - i)?;

        let mut credits_earned = 0;
        let credits_earned = middle_prop.request.calc_cost(request_len)?
            .checked_add(middle_prop.response.calc_cost(
                response_len.checked_add(max_failure_len)?)?)?;

        sum_credits = sum_credits.checked_add(credits_earned)?;
    }

    Some(sum_credits)
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

/*
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

*/
