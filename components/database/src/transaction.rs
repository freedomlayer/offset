use std::fmt::Debug;

use futures::future::BoxFuture;
use futures::Future;

/// A database transaction. Enforced using closure syntax.
/// Supports nested transactions.
trait Transaction {
    /// Begin a new transaction.
    /// Transaction ends at the end of the closure scope.
    fn transaction<'a, F, FR, T, E>(&'a mut self, f: F) -> BoxFuture<'a, Result<T, E>>
    where
        F: (FnOnce(&'a mut Self) -> FR) + Send + 'a,
        FR: Future<Output = Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: Debug + Send + 'a;
}
