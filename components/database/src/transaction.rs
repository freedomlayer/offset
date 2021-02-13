use futures::future::BoxFuture;

// TODO: This is a compromise. We would have preferred to use a closure, but the lifetimes with the
// Transaction traits seem to not work out.
// See: https://users.rust-lang.org/t/returning-this-value-requires-that-1-must-outlive-2/51417
/// A transaction function
pub trait TransFunc {
    /// Input (Will be handed as shared reference)
    type InRef;
    /// Output
    type Out;
    /// Call invocation
    fn call<'a>(self, input: &'a mut Self::InRef) -> BoxFuture<'a, Self::Out>
    where
        Self: 'a;
}

/// A database transaction. Enforced using closure syntax.
/// Supports nested transactions.
pub trait Transaction
where
    Self: std::marker::Sized,
{
    /// Begin a new transaction.
    /// Transaction ends at the end of the closure scope.
    /// If the returned value is Ok(..), the transaction was successful. Otherwise, the transaction
    /// was canceled.
    fn transaction<'a, F, T, E>(&'a mut self, f: F) -> BoxFuture<'a, Result<T, E>>
    where
        F: TransFunc<InRef = Self, Out = Result<T, E>> + Send + 'a,
        T: Send,
        E: Send;
}
