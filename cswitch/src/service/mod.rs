//! Definition of the core infrastructure to internal service.

#![allow(dead_code)]

use futures::Future;

/// The internal service state.
pub enum ServiceState<E> {
    Living,
    Closing(Box<Future<Item=(), Error=E>>),
    Empty
}

pub trait Service {
    /// Requests handled by the service.
    type Request;
    /// Response given by the service.
    type Response;
    /// Errors produced by the service.
    type Error;
    /// The future response value.
    type Future: Future<Item=Self::Response, Error=Self::Error>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call `poll_ready`. If the
    /// service is at capacity and the request is unable to be handled, the
    /// returned `Future` should resolve to an error.
    fn call(&mut self, request: Self::Request) -> Self::Future;
}

