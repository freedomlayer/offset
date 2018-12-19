use std::marker::PhantomData;

use crypto::hash::{HashResult, HASH_RESULT_LEN};
use super::verifier::Verifier;


struct DummyVerifier<N,B,U> {
    phantom_n: PhantomData<N>,
    phantom_b: PhantomData<B>,
    phantom_u: PhantomData<U>,
    hash_vec: Vec<HashResult>,
}

impl<N,B,U> DummyVerifier<N,B,U> {
    pub fn new() -> Self {
        Self {
            phantom_n: PhantomData,
            phantom_b: PhantomData,
            phantom_u: PhantomData,
            // verify() method requires to return a borrowed slice, therefore
            // we need to keep a real vector:
            hash_vec: vec![HashResult::from(&[0; HASH_RESULT_LEN]),
                           HashResult::from(&[1; HASH_RESULT_LEN]),
                           HashResult::from(&[2; HASH_RESULT_LEN])],
        }
    }
}

impl<N,B,U> Verifier for DummyVerifier<N,B,U> {
    type Node = N;
    type Neighbor = B;
    type SessionId = U;

    fn verify(&mut self, 
               origin_tick_hash: &HashResult,
               expansion_chain: &[&[HashResult]],
               node: &N,
               session_id: &U,
               counter: u64) -> Option<&[HashResult]> {
        
        // Everything is successfully verified:
        Some(&self.hash_vec)
    }

    fn tick(&mut self) -> (HashResult, Vec<N>) {
        // Nothing happens. Always the same tick.
        (HashResult::from(&[0; HASH_RESULT_LEN]), Vec::new())
    }

    fn neighbor_tick(&mut self, neighbor: B, tick_hash: HashResult) -> Option<HashResult> {
        // Nothing happens
        None
    }

    fn remove_neighbor(&mut self, neighbor: &B) -> Option<HashResult> {
        // Nothing happens
        None
    }
}
