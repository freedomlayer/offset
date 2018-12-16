use crypto::hash::HashResult;

pub trait Verifier {
    type Node;
    type SessionId;

    /// Verify an incoming message:
    /// - Checks freshness using a chain of time hashes.
    /// - Making sure that the message is not out of order using a ratchet counter.
    fn verify(&mut self, 
                   origin_tick_hash: &HashResult,
                   expansion_chain: &[Vec<HashResult>],
                   node: &Self::Node,
                   session_id: &Self::SessionId,
                   counter: u64) -> Option<Vec<HashResult>>;

    /// One time tick. Returns a `tick_hash` representing the local current time,
    /// and a vector of all the nodes removed due to timeout
    fn tick(&mut self) -> (HashResult, Vec<Self::Node>);

    /// Process a time tick from a neighbor. This information is used when producing a `tick_hash`.
    fn neighbor_tick(&mut self, neighbor: Self::Node, tick_hash: HashResult) -> Option<HashResult>;

    /// Remove a neighbor. This method should be invoked when a neighbor disconnects.
    /// If not called, the time proofs (list of hashes) will be larger than needed.
    fn remove_neighbor(&mut self, neighbor: &Self::Node) -> Option<HashResult>;
}

