pub trait MutableState {
    type Mutation;
    type MutateError;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError>;
}
