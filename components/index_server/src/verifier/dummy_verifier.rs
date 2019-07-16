use std::marker::PhantomData;

use super::verifier::Verifier;
use proto::crypto::HashResult;

pub struct DummyVerifier<N, B, U> {
    phantom_n: PhantomData<N>,
    phantom_b: PhantomData<B>,
    phantom_u: PhantomData<U>,
    hash_vec: Vec<HashResult>,
}

impl<N, B, U> DummyVerifier<N, B, U> {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            phantom_n: PhantomData,
            phantom_b: PhantomData,
            phantom_u: PhantomData,
            // verify() method requires to return a borrowed slice, therefore
            // we need to keep a real vector:
            hash_vec: vec![
                HashResult::from(&[0; HashResult::len()]),
                HashResult::from(&[1; HashResult::len()]),
                HashResult::from(&[2; HashResult::len()]),
            ],
        }
    }
}

impl<N, B, U> Verifier for DummyVerifier<N, B, U> {
    type Node = N;
    type Neighbor = B;
    type SessionId = U;

    fn verify(
        &mut self,
        _origin_tick_hash: &HashResult,
        _expansion_chain: &[&[HashResult]],
        _node: &N,
        _session_id: &U,
        _counter: u64,
    ) -> Option<&[HashResult]> {
        // Everything is successfully verified:
        Some(&self.hash_vec)
    }

    fn tick(&mut self) -> (HashResult, Vec<N>) {
        // Nothing happens. Always the same tick.
        (HashResult::from(&[0; HashResult::len()]), Vec::new())
    }

    fn neighbor_tick(&mut self, _neighbor: B, _tick_hash: HashResult) -> Option<HashResult> {
        // Nothing happens
        None
    }

    fn remove_neighbor(&mut self, _neighbor: &B) -> Option<HashResult> {
        // Nothing happens
        None
    }
}
