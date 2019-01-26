use funder::{FunderState, FunderMutation};
use index_client::{IndexClientConfig, IndexClientConfigMutation};
use database::atomic_db::MutableState;

pub enum NodeMutation<B,ISA> {
    Funder(FunderMutation<Vec<B>>),
    IndexClient(IndexClientConfigMutation<ISA>)
}

pub struct NodeState<B: Clone,ISA> {
    pub funder: FunderState<B>,
    pub index_client: IndexClientConfig<ISA>,
}

/*
impl NodeState {
    pub fn new() -> Self {
        NodeState {
            funder: FunderState::new(),
            index_client: IndexClientConfig::new(),
        }
    }
}


pub enum NodeMutateError {
    // TODO: Add internal cause?
    FunderMutateError,
}

impl<B,ISA> MutableState for NodeState<B,ISA> {
    type Mutation = NodeMutation<B,ISA>;
    type MutateError = NodeMutateError;

    fn initial() -> Self {
        NodeState::new()
    }

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        match mutation {
            NodeMutation::Funder(funder_mutation) => {
                self.funder.mutate(funder_mutation)
                    .map_err(|_| NodeMutateError::FunderMutateError)
            },
            NodeMutation::IndexClient(index_client_mutation) => {
                self.index_client.mutate(index_client_mutation);
                Ok(())
            },
        }
    }
}
*/
