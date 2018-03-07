use super::super::messages::PendingNeighborRequest;



pub struct CreditCalculator {

}

impl CreditCalculator{
    // TODO(a4vision): pass the functions only the required parameters.
    pub fn credit_on_sucess(&self, request: &PendingNeighborRequest) -> u64{
        0
    }

    pub fn pending_credit(&self, request: &PendingNeighborRequest) -> u64{
        0
    }

    pub fn credit_on_failure(&self, request: &PendingNeighborRequest)-> u64{
        0
    }
}
