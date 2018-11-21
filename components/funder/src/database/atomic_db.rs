
/// An atomic database. Allows to batch a list of mutations, and guarantees to apply them to the
/// database in an atomic manner.
pub trait AtomicDb {
    type State;
    type Mutation;
    type Error;

    fn get_state(&self) -> &Self::State;
    fn mutate(&mut self, mutations: Vec<Self::Mutation>) -> Result<(), Self::Error>;
}
