use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;


pub trait Verifier {
    type Node;
    type SessionId;

    fn verify(&mut self, 
                   origin_tick_hash: &HashResult,
                   expansion_chain: &[Vec<HashResult>],
                   node: &Self::Node,
                   session_id: &Self::SessionId,
                   counter: u64) -> Option<Vec<HashResult>>;

    fn tick(&mut self, rand_value: RandValue) -> HashResult;
    fn neighbor_tick(&mut self, neighbor: Self::Node, tick_hash: HashResult) -> Option<HashResult>;
    fn remove_neighbor(&mut self, neighbor: &Self::Node) -> Option<HashResult>;
}

