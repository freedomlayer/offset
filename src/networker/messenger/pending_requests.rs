
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

impl PendingRequests{
    pub fn new(local_requests: HashMap<Uid, PendingNeighborRequest>, remote_requests: HashMap<Uid, PendingNeighborRequest>,)
    -> PendingRequests{
        PendingRequests{
            pending_local_requests: local_requests,
            pending_remote_requests: remote_requests,
        }
    }

    fn vec_to_hashmap(requests: Vec<PendingNeighborRequest>)
        -> HashMap<Uid, PendingNeighborRequest>{
        let mut hashmap = HashMap::new();
        for request in requests{
            hashmap.insert(request.request_id, request);
        }
        hashmap
    }

    pub fn from_vecs_ignore_duplicates(local: Vec<PendingNeighborRequest>, remote: Vec<PendingNeighborRequest>)
    -> PendingRequests {
        PendingRequests::new(PendingRequests::vec_to_hashmap(local),
                             PendingRequests::vec_to_hashmap(remote))
    }
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

    #[allow(map_entry)]
    pub fn add_pending_request(&mut self, pending_request: PendingNeighborRequest) -> bool {
        if self.tp_requests.get_hmap().contains_key(&pending_request.request_id) {
            false
        } else {
            self.tp_requests.insert(pending_request.request_id, pending_request);
            true
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

