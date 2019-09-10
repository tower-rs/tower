use crate::error::Error;
use std::future::Future;

/// Checks a request
pub trait Predicate<Request> {
    type Future: Future<Output = Result<(), Error>>;

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
