use crypto::identity::{PublicKey, Signature};

pub enum IndexingProviderStatus {
    Enabled = 1,
    Disabled = 0,
}

pub struct IndexingProviderInfo {
    pub id: IndexingProviderId,
    pub state_chain_link: StateChainLink,
}

pub struct IndexingProviderInfoFromDB {
    id: IndexingProviderId,
    state_chain_link: StateChainLink,
    last_routes: Vec<NeighborsRoute>,
    status: IndexingProviderStatus,
}