
pub trait AtomicDb {
    type State;
    type Mutation;
    type Error;

    fn get_state(&self) -> &Self::State;
    fn mutate(&mut self, mutations: Vec<Self::Mutation>) -> Result<(), Self::Error>;
}
