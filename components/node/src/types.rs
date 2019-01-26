use common::canonical_serialize::CanonicalSerialize;

use crypto::identity::PublicKey;
use funder::{FunderState, FunderMutation};
use index_client::{IndexClientConfig, IndexClientConfigMutation};
use database::atomic_db::MutableState;

pub enum NodeMutation<B,ISA> {
    Funder(FunderMutation<Vec<B>>),
    IndexClient(IndexClientConfigMutation<ISA>)
}

pub struct NodeState<B: Clone,ISA> {
    pub funder: FunderState<Vec<B>>,
    pub index_client: IndexClientConfig<ISA>,
}

impl<B,ISA> NodeState<B,ISA> 
where
    B: Clone + CanonicalSerialize,
{
    pub fn new(local_public_key: PublicKey) -> Self {
        let relay_addresses = Vec::new();
        NodeState {
            funder: FunderState::new(&local_public_key, &relay_addresses),
            index_client: IndexClientConfig::new(),
        }
    }
}

/*

pub enum NodeMutateError {
}

impl<B,ISA> MutableState for NodeState<B,ISA> 
where
    B: Clone + CanonicalSerialize,
{
    type Mutation = NodeMutation<B,ISA>;
    type MutateError = NodeMutateError;

    fn initial() -> Self {
        NodeState::new()
    }

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            NodeMutation::Funder(funder_mutation) => {
                self.funder.mutate(funder_mutation);
                Ok(())
            },
            NodeMutation::IndexClient(index_client_mutation) => {
                self.index_client.mutate(index_client_mutation);
                Ok(())
            },
        }
    }
}
*/
