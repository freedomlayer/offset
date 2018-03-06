use std::collections::HashMap;
use utils::trans_hashmap_mut::TransHashMapMut;
use crypto::uid::Uid;
use super::super::messages::PendingNeighborRequest;


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
}
