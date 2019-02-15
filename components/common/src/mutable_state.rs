
pub trait MutableState {
    type Mutation;
    type MutateError;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError>;
}

/// Given a type which is a MutableState, we convert it to a type
/// that is a MutableState for batches of mutations of the same mutation type.
#[derive(Debug, Clone)]
pub struct BatchMutable<T>(pub T);

impl<T,M,E> MutableState for BatchMutable<T>
where
    T: MutableState<Mutation=M,MutateError=E>,
{
    type Mutation = Vec<M>;
    type MutateError = E;

    fn mutate(&mut self, mutations: &Self::Mutation) -> Result<(), Self::MutateError> {
        for mutation in mutations {
            self.0.mutate(mutation)?;
        }
        Ok(())
    }
}
