use super::error::Error;
use std::future::Future;

/// Checks a request
pub trait Predicate<Request> {
    /// The future returned by `check`.
    type Future: Future<Output = Result<(), Error>>;

    /// Check whether the given request should be forwarded.
    ///
    /// If the future resolves with `Ok`, the request is forwarded to the inner service.
    fn check(&mut self, request: &Request) -> Self::Future;
}

impl<F, T, U> Predicate<T> for F
where
    F: Fn(&T) -> U,
    U: Future<Output = Result<(), Error>>,
{
    type Future = U;

    fn check(&mut self, request: &T) -> Self::Future {
        self(request)
    }
}
