use utils::crypto::identity::{PublicKey, Signature};

pub const INDEXING_PROVIDER_STATE_HASH_LEN: usize = 32;
pub const INDEXING_PROVIDER_ID_LEN: usize = 16;

// A hash of a full link in an indexing provider chain
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct IndexingProviderStateHash([u8; INDEXING_PROVIDER_STATE_HASH_LEN]);

impl IndexingProviderStateHash {
    pub fn from_bytes<T>(t: &T) -> Result<Self, ()>
        where T: AsRef<[u8]>
    {
        let in_bytes = t.as_ref();

        if in_bytes.len() != INDEXING_PROVIDER_STATE_HASH_LEN {
            Err(())
        } else {
            let mut state_hash_bytes = [0; INDEXING_PROVIDER_STATE_HASH_LEN];
            state_hash_bytes.clone_from_slice(in_bytes);
            Ok(IndexingProviderStateHash(state_hash_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for IndexingProviderStateHash {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

// The name of an indexing provider.
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct IndexingProviderId([u8; INDEXING_PROVIDER_ID_LEN]);

impl IndexingProviderId {
    pub fn from_bytes<T>(t: &T) -> Result<Self, ()>
        where T: AsRef<[u8]>
    {
        let in_bytes = t.as_ref();

        if in_bytes.len() != INDEXING_PROVIDER_ID_LEN {
            Err(())
        } else {
            let mut provider_id_bytes = [0; INDEXING_PROVIDER_ID_LEN];
            provider_id_bytes.clone_from_slice(in_bytes);
            Ok(IndexingProviderId(provider_id_bytes))
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

impl AsRef<[u8]> for IndexingProviderId {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

pub struct IndexingProviderInfo {
    pub id: IndexingProviderId,
    pub state_chain_link: StateChainLink,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct NeighborsRoute {
    pub public_keys: Vec<PublicKey>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct FriendsRoute {
    pub public_keys: Vec<PublicKey>,
    // How much credit can we push through this route?
    pub capacity: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct StateChainLink {
    pub previous_state_hash: IndexingProviderStateHash,
    pub new_owners_public_keys: Vec<PublicKey>,
    pub new_indexers_public_keys: Vec<PublicKey>,
    pub signatures_by_old_owners: Vec<Signature>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct IndexerRoute {
    pub neighbors_route: NeighborsRoute,
    pub app_port: u32,
}