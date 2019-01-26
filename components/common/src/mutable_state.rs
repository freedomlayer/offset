
pub trait MutableState {
    type InitialArg;
    type Mutation;
    type MutateError;

    fn initial(initial_arg: Self::InitialArg) -> Self;
    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError>;
}
