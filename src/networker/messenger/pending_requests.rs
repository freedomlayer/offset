use std::collections::HashMap;
use utils::trans_hashmap_mut::TransHashMapMut;
use crypto::uid::Uid;
use crypto::identity::PublicKey;
use proto::indexer::PkPairPosition;
use super::super::messages::PendingNeighborRequest;

// TODO(a4vision): Note that the functions for local requests and for
// local requests are probably identical. Maybe hold each one of them in a separate instance
// of this struct.
pub struct PendingRequests{
    pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

pub struct TransPendingRequests<'a>{
    tp_local_requests: TransHashMapMut<'a, Uid, PendingNeighborRequest>,
    tp_remote_requests: TransHashMapMut<'a, Uid, PendingNeighborRequest>,
}


impl <'a> TransPendingRequests<'a> {
    pub fn new(pending_requests: &'a mut PendingRequests) -> Self {
        TransPendingRequests {
            tp_local_requests: TransHashMapMut::new(&mut pending_requests.pending_local_requests),
            tp_remote_requests: TransHashMapMut::new(&mut pending_requests.pending_remote_requests),
        }
    }

    pub fn cancel(self) {
        let TransPendingRequests{tp_local_requests, tp_remote_requests } = self;
        tp_local_requests.cancel();
        tp_remote_requests.cancel();
    }

    // TODO(a4vision): Is it reasonable to consume the request here ?
    pub fn add_pending_remote_request(&mut self, remote_request: PendingNeighborRequest) -> bool {
        if self.tp_remote_requests.get_hmap().contains_key(&remote_request.request_id){
            return false;
        }else{
            self.tp_remote_requests.insert(remote_request.request_id, remote_request);
            return  true;
        }
    }

    /// Total amount of remote pending credit towards the given neighbor
    pub fn get_total_remote_pending_to(&self, local_public_key: &PublicKey, remote_public_key: &PublicKey) -> u64 {
        assert!(false);
        let mut total: u64 = 0;
        for request in self.tp_remote_requests.get_hmap().values() {
            let position = request.route.find_pk_pair(&local_public_key, &remote_public_key);
            if position != PkPairPosition::NotFound{
                // total += calculator.pending_credit(&request);
                // TODO
            }
        }
        return total;
    }
}

