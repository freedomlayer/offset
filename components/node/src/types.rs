use funder::FunderMutation;
use index_client::IndexClientConfigMutation;

pub enum NodeMutation<B,ISA> {
    Funder(FunderMutation<Vec<B>>),
    IndexClient(IndexClientConfigMutation<ISA>)
}
