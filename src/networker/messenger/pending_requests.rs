
use std::collections::HashMap;
use utils::trans_hashmap_mut::TransHashMapMut;
use crypto::uid::Uid;
use crypto::identity::PublicKey;
use proto::indexer::PkPairPosition;
use super::pending_neighbor_request::PendingNeighborRequest;
use super::messenger_messages::RequestSendMessage;

pub struct PendingRequests{
    pending_local_requests: HashMap<Uid, PendingNeighborRequest>,
    pending_remote_requests: HashMap<Uid, PendingNeighborRequest>,
}

pub struct TransPendingRequests<'a>{
    tp_requests: TransHashMapMut<'a, Uid, PendingNeighborRequest>,
}


impl <'a> TransPendingRequests<'a> {
    pub fn new_transactionals(pending_requests: &'a mut PendingRequests) -> (Self, Self) {
        (TransPendingRequests {
            tp_requests: TransHashMapMut::new(&mut pending_requests.pending_local_requests)},
            TransPendingRequests {
            tp_requests: TransHashMapMut::new(&mut pending_requests.pending_remote_requests)})
    }

    pub fn cancel(self) {
        self.tp_requests.cancel();
    }

    pub fn add_pending_request(&mut self, pending_request: PendingNeighborRequest) -> bool {
        if self.tp_requests.get_hmap().contains_key(&pending_request.request_id) {
            return false;
        } else {
            self.tp_requests.insert(pending_request.request_id.clone(), pending_request);
            return true;
        }
    }

    pub fn remove_pending_request(&mut self, uid: &Uid) -> Option<PendingNeighborRequest>{
        self.tp_requests.remove(uid)
    }


    /*
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
    */
}

