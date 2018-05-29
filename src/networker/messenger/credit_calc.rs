use std::convert::TryFrom;
use std::{mem, cmp};
use crypto::identity::{PublicKey, Signature};
use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use proto::indexer::{PaymentProposalPair, NeighborRouteLink};
use proto::networker::NetworkerSendPrice;
use utils::int_convert::usize_to_u32;
use super::messenger_messages::NetworkerFreezeLink;

pub struct PaymentProposals {
    middle_props: Vec<PaymentProposalPair>,
    dest_response_proposal: NetworkerSendPrice,
}


/// nodes_to_dest = 0 means we are the dest node.
/// Example:
/// ```text
///                         r->
///    B  --   C   --   D   --   E   --   F   
/// ```
/// route_len = 5
/// nodes_to_dest = 2
///
/// route_len >= 2
/// nodes_to_dest < route_len
fn calc_request_len(request_content_len: u32, 
                    route_len: u32, 
                    nodes_to_dest: u32) -> Option<u32> {

    let public_key_len = usize_to_u32(mem::size_of::<PublicKey>())?;
    let networker_send_price_len = usize_to_u32(mem::size_of::<NetworkerSendPrice>())?;
    let networker_freeze_link_len = usize_to_u32(mem::size_of::<NetworkerFreezeLink>())?;
    let neighbor_route_link_len = usize_to_u32(mem::size_of::<NeighborRouteLink>())?;

    let route_bytes_count = public_key_len.checked_mul(2)?
        .checked_add(route_len.checked_sub(2)?.checked_mul(neighbor_route_link_len)?)?
        .checked_add(networker_send_price_len.checked_mul(2)?)?;

    let freeze_links_len = route_len.checked_sub(nodes_to_dest)?
        .checked_mul(networker_freeze_link_len)?;

    let request_overhead = usize_to_u32(mem::size_of::<Uid>())?
        .checked_add(usize_to_u32(mem::size_of::<u32>())?)?
        .checked_add(usize_to_u32(mem::size_of::<u64>())?)?;

    Some(request_overhead
         .checked_add(route_bytes_count)?
         .checked_add(request_content_len)?
         .checked_add(freeze_links_len)?)
}

fn calc_response_len(response_content_len: u32) -> Option<u32> {
    Some(usize_to_u32(mem::size_of::<Uid>())?
        .checked_add(usize_to_u32(mem::size_of::<RandValue>())?)?
        .checked_add(usize_to_u32(mem::size_of::<u64>())?)?
        .checked_add(response_content_len)?
        .checked_add(usize_to_u32(mem::size_of::<Signature>())?)?)
}

/// nodes_to_reporting = 0 means we are the reporting node.
///
/// Example:
///
/// ```text
///        <-f                  rep
///    B   --   C   --   D   --  E   --   F
/// ```
/// nodes_to_reporting = 2
///
fn calc_failure_len(nodes_to_reporting: u32) -> Option<u32> {
    let rand_nonce_sig_len = usize_to_u32(mem::size_of::<RandValue>())?
        .checked_add(usize_to_u32(mem::size_of::<Signature>())?)?;

    Some(usize_to_u32(mem::size_of::<Uid>())?
        .checked_add(usize_to_u32(mem::size_of::<u16>())?)?
        .checked_add(
            rand_nonce_sig_len.checked_mul(nodes_to_reporting)?)?)
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
fn credits_on_success_dest(payment_proposals: &PaymentProposals,
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

    let resp_prop = &payment_proposals.dest_response_proposal;
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


/// Amount of credit paid to a node that sent a valid Response
///
/// ```text
///           req      req      req
///           res      res      res      res
///    B  --  (C)  --   D   --   E   --   F   
/// ```
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
    let middle_props_len = usize_to_u32(middle_props.len())?;

    let mut sum_credits: u64 = credits_on_success_dest(payment_proposals,
                                                       processing_fee_proposal,
                                                       response_content_len,
                                                       max_response_content_len)?;

    /*
                                 middle_props
                                |-----------|
                            B -- C -- D -- E -- F
      nodes_to_dest:        4    3    2    1    0

    */

    for i in middle_props_len.checked_sub(nodes_to_dest)? .. middle_props_len {
        let middle_prop = &middle_props[i as usize];

        // TODO; Check for off by one here:
        let request_len = calc_request_len(request_content_len,
                                           middle_props_len,
                                           middle_props_len.checked_sub(i)?)?;
        let response_len = calc_response_len(response_content_len)?;
        // Maximum failure length occurs when the reporting node is as far as possible.  Note that
        // here we pick the last node to be the reporting node. This can't really happen.
        let max_failure_len = calc_failure_len(middle_props_len.checked_sub(i)?)?;

        let mut credits_earned = 0;
        let credits_earned = middle_prop.request.calc_cost(request_len)?
            .checked_add(middle_prop.response.calc_cost(
                response_len.checked_add(max_failure_len)?)?)?;

        sum_credits = sum_credits.checked_add(credits_earned)?;
    }

    Some(sum_credits)
}

/// The amount of credits paid to a node in case of failure.
///
/// ```text
///           req      req      req
///           res      res      res      res
///    B  --  (C)  --   D   --   E   --   F   
///                           reporting
///    0       1        2        3        4
/// ```
///
/// Examples:
/// Node D (nonreporting) should earn:
///
/// ```text
///     {CD}_b + request_len * {CD}_r 
///         + {CB}_b + failure_len * {CB}_r 
/// ```
///
/// Node E (reporting) should earn:
///
/// ```text
///     {ED}_b + failure_len * {ED}_r
/// ```
///
/// Because the node E did not pass the message on.
///
pub fn credits_on_failure(payment_proposals: &PaymentProposals,
                          request_content_len: u32, 
                          nodes_to_reporting: u32,
                          reporting_to_dest: u32) -> Option<u64> {

    // Dest node can never report a failure:
    assert!(reporting_to_dest > 1);

    // TODO: Fix all 'as usize' in this function.
    let middle_props = &payment_proposals.middle_props;
    let middle_props_len = usize_to_u32(middle_props.len())?;

    let mut sum_credits: u64 = 0;
    let end_index = middle_props_len.checked_sub(reporting_to_dest)?;
    let start_index = end_index.checked_sub(nodes_to_reporting)?;

    for i in start_index .. end_index {
        let middle_prop = &middle_props[i as usize];
        // TODO: Check for off by one here:
        let request_len = calc_request_len(request_content_len,
                                           middle_props_len,
                                           middle_props_len.checked_sub(i)?)?;
        let failure_len = calc_failure_len(end_index.checked_sub(i)?)?;

        let credits_earned = middle_prop.request.calc_cost(request_len)?
            .checked_add(middle_prop.response.calc_cost(failure_len)?)?;

        sum_credits = sum_credits.checked_add(credits_earned)?;
    }

    // Add the payment for the reporting node.
    // This is the special case of i = end_index :
    let failure_len_reporting = calc_failure_len(0)?;
    let reporting_node_credits = middle_props[end_index as usize].response
        .calc_cost(failure_len_reporting)?;
    sum_credits = sum_credits.checked_add(reporting_node_credits)?;

    Some(sum_credits)
}

/// Compute the amount of credits we need to freeze.
///
/// ```text
///           req      req      req
///           res      res      res      res
///    B  --  (C)  --   D   --   E   --   F   
/// ```
///
pub fn credits_to_freeze(payment_proposals: &PaymentProposals,
                          processing_fee_proposal: u64,
                          request_content_len: u32,
                          max_response_content_len: u32,
                          nodes_to_dest: u32) -> Option<u64> {
    
    // Maximum is obtained when response_content_len = 0.
    // TODO: Check this.
    credits_on_success(payment_proposals,
                       processing_fee_proposal,
                       request_content_len,
                       0,
                       max_response_content_len,
                       nodes_to_dest)
}


#[cfg(test)]
mod tests {
    use super::*;
    use proto::networker::LinearSendPrice;
    use num_traits::PrimInt;

    #[test]
    fn test_calc_request_len_basic() {
        /*
            B   --   C
        */
        let request_content_len = 4u32;
        let route_len = 2;
        let nodes_to_dest = 1;
        let opt_request_len = calc_request_len(request_content_len,
                         route_len,
                         nodes_to_dest);

        let neighbors_route_len = 
            2 * mem::size_of::<PublicKey>()
            /* +  0 * mem::size_of::<NeighborRouteLink>() */
            + 2 * mem::size_of::<NetworkerSendPrice>();

        let freeze_link_len = mem::size_of::<NetworkerFreezeLink>();

        let expected_request_len = mem::size_of::<Uid>()  // requestId
            + mem::size_of::<u32>()  // maxResponseLen
            + mem::size_of::<u64>()  // processingFeeProposal
            + neighbors_route_len 
            + (request_content_len as usize)
            + freeze_link_len /* * 1 */;

        assert_eq!(opt_request_len, Some(expected_request_len as u32))

    }

    fn is_linear<F,N,M>(f: F, begin: N, end: N) -> bool
    where 
        F: Fn(N) -> M,
        N: PrimInt,
        M: PrimInt,
    {

        let n_one = N::one();
        let n_two = n_one + n_one;
        assert!(end >= begin + n_two);
        let mut x = begin;
        while x < end - n_two {
            if f(x + n_two) + f(x) != f(x + n_one) + f(x + n_one) {
                return false
            }
            x = x + n_one;
        }
        true
    }

    #[test]
    fn test_calc_request_len_linearity() {
        let num_iters = 15;

        for nodes_to_dest in 0 .. num_iters {
            for route_len in cmp::max(2, nodes_to_dest + 1) .. nodes_to_dest + num_iters {
                let f = |request_content_len| calc_request_len(request_content_len, 
                                                                route_len, 
                                                                nodes_to_dest).unwrap();
                assert!(is_linear(f, 0, num_iters))
            }
        }
        for request_content_len in 0 .. num_iters {
            for nodes_to_dest in 0 .. num_iters {
                let f = |route_len| calc_request_len(request_content_len, 
                                                     route_len, 
                                                     nodes_to_dest).unwrap();
                assert!(is_linear(f, cmp::max(2, nodes_to_dest + 1), nodes_to_dest + num_iters + 1))
            }
        }
        for request_content_len in 0 .. num_iters {
            for route_len in 3 .. num_iters {
                let f = |nodes_to_dest| calc_request_len(request_content_len, 
                                                     route_len, 
                                                     nodes_to_dest).unwrap();
                assert!(is_linear(f, 0, route_len - 1))
            }
        }
    }

    #[test]
    fn test_calc_response_len_basic() {
        let response_content_len = 5;
        let opt_response_len = calc_response_len(response_content_len);

        let expected_response_len = 
            mem::size_of::<Uid>()
                + mem::size_of::<RandValue>() 
                + mem::size_of::<u64>()
                + (response_content_len as usize)
                + mem::size_of::<Signature>();

        assert_eq!(opt_response_len, Some(expected_response_len as u32));
    }

    #[test]
    fn test_calc_response_len_linearity() {
        let f = |response_content_len| 
                  calc_response_len(response_content_len).unwrap();
        assert!(is_linear(f, 0, 100));
    }

    #[test]
    fn test_calc_failure_len_basic() {
        /*
                <-f                reporting
            A   --   B   --   C   --   D   ...
            
            We are B, D is the reporting node.
        */
        let nodes_to_rep = 2;
        let opt_failure_len = calc_failure_len(nodes_to_rep);

        let rand_nonce_sig_len = 
            mem::size_of::<RandValue>() + mem::size_of::<Signature>();

        let expected_failure_len = mem::size_of::<Uid>()
                + mem::size_of::<u16>()
                + rand_nonce_sig_len * (nodes_to_rep as usize);

        assert_eq!(opt_failure_len, Some(expected_failure_len as u32));

    }

    #[test]
    fn test_calc_failure_len_linearity() {
        let f = |nodes_to_rep| 
                  calc_failure_len(nodes_to_rep).unwrap();
        assert!(is_linear(f, 0, 100));
    }


    /// Short function for generating a NetworkerSendPrice (base, multiplier)
    fn send_price(base: u32, multiplier: u32) -> NetworkerSendPrice {
        NetworkerSendPrice(LinearSendPrice {
            base,
            multiplier,
        })
    }

    fn example_payment_proposals() -> PaymentProposals {
        PaymentProposals {
            middle_props: vec![
                PaymentProposalPair { request: send_price(1,2), response: send_price(4,3) },
                PaymentProposalPair { request: send_price(2,3), response: send_price(1,5) },
                PaymentProposalPair { request: send_price(3,2), response: send_price(2,5) },
                PaymentProposalPair { request: send_price(6,7), response: send_price(9,6) },
                PaymentProposalPair { request: send_price(3,4), response: send_price(3,4) },
            ],
            dest_response_proposal: send_price(2,3),
        }
    }

    /// credits_on_success() with nodes_to_dest = 0 should have the same result as
    /// credits_on_success_dest().
    #[test]
    fn test_credits_on_success_nodes_to_dest_zero() {
        let payment_proposals = example_payment_proposals();
        let processing_fee_proposal = 10u64;
        let request_content_len = 30u32;
        let response_content_len = 20u32;
        let max_response_content_len = 40u32;
        let c_on_success = credits_on_success(&payment_proposals,
                                      processing_fee_proposal,
                                      request_content_len,
                                      response_content_len,
                                      max_response_content_len,
                                      0).unwrap();

        let c_on_success_dest = credits_on_success_dest(&payment_proposals,
                                                        processing_fee_proposal,
                                                        response_content_len,
                                                        max_response_content_len).unwrap();
        assert_eq!(c_on_success, c_on_success_dest);
    }

    #[test]
    fn credits_on_success_dest_linearity() {
        let payment_proposals = example_payment_proposals();
        for response_content_len in 0 .. 20 {
            for max_response_content_len in response_content_len .. response_content_len + 20 {
                let f = |processing_fee_proposal| credits_on_success_dest(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    response_content_len,
                                                    max_response_content_len).unwrap();
                assert!(is_linear(f, 0, 20));
            }
        }

        for processing_fee_proposal in 0 .. 20 {
            for max_response_content_len in 2 .. 20 {
                let f = |response_content_len| credits_on_success_dest(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    response_content_len,
                                                    max_response_content_len).unwrap();
                assert!(is_linear(f, 0, max_response_content_len));
            }
        }

        for processing_fee_proposal in 0 .. 20 {
            for response_content_len in 0 .. 20 {
                let f = |max_response_content_len| credits_on_success_dest(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    response_content_len,
                                                    max_response_content_len).unwrap();
                assert!(is_linear(f, response_content_len, response_content_len + 20));
            }
        }
    }

    #[test]
    fn tests_credits_on_success_dest_basic() {
        // TODO
        let payment_proposals = example_payment_proposals();
        let processing_fee_proposal = 10u64;
        let response_content_len = 20u32;
        let max_response_content_len = 40u32;
        let opt_credits = credits_on_success_dest(&payment_proposals,
                                processing_fee_proposal,
                                response_content_len,
                                max_response_content_len);
        assert!(!opt_credits.is_none());
    }

    #[test]
    fn credits_on_success_linearity() {
        let payment_proposals = example_payment_proposals();
        let route_len = (payment_proposals.middle_props.len() + 2) as u32;

        // ... processing_fee_proposal ...
        for request_content_len in 0 .. 5 {
        for max_response_content_len in 2 .. 5 {
        for response_content_len in 0 .. max_response_content_len {
        for nodes_to_dest in 0 .. route_len - 1 {
            let f = |processing_fee_proposal| credits_on_success(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    request_content_len,
                                                    response_content_len,
                                                    max_response_content_len,
                                                    nodes_to_dest).unwrap();
            assert!(is_linear(f, 0, 20));
        }
        }
        }
        }

        for processing_fee_proposal in 0 .. 5 {
        // ... request_content_len ...
        for max_response_content_len in 2 .. 5 {
        for response_content_len in 0 .. max_response_content_len {
        for nodes_to_dest in 0 .. route_len - 1 {
            let f = |request_content_len| credits_on_success(
                                                &payment_proposals,
                                                processing_fee_proposal,
                                                request_content_len,
                                                response_content_len,
                                                max_response_content_len,
                                                nodes_to_dest).unwrap();
            assert!(is_linear(f, 0, 15));
        }
        }
        }
        }

        for processing_fee_proposal in 0 .. 8 {
        for request_content_len in 0 .. 8 {
        // ... response_content_len ...
        for max_response_content_len in 2 .. 5 {
        for nodes_to_dest in 0 .. route_len - 1 {
            let f = |response_content_len| credits_on_success(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    request_content_len,
                                                    response_content_len,
                                                    max_response_content_len,
                                                    nodes_to_dest).unwrap();
            assert!(is_linear(f, 0, max_response_content_len));
        }
        }
        }
        }

        for processing_fee_proposal in 0 .. 6 {
        for request_content_len in 0 .. 6 {
        for response_content_len in 0 .. 6 {
        // ... max_response_content_len ...
        for nodes_to_dest in 0 .. route_len - 1 {
            let f = |max_response_content_len| credits_on_success(
                                                    &payment_proposals,
                                                    processing_fee_proposal,
                                                    request_content_len,
                                                    response_content_len,
                                                    max_response_content_len,
                                                    nodes_to_dest).unwrap();
            assert!(is_linear(f, response_content_len, response_content_len + 15));
        }
        }
        }
        }

        // credits_on_success is not guaranteed to be linear on nodes_to_dest.
        // Therefore we don't test for this property here.
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

