use crate::error::Error;
use futures::{Future, IntoFuture};

/// Checks a request
pub trait Predicate<Request> {
    type Future: Future<Item = (), Error = Error>;

    fn check(&mut self, request: &Request) -> Self::Future;
}

impl<F, T, U> Predicate<T> for F
where
    F: Fn(&T) -> U,
    U: IntoFuture<Item = (), Error = Error>,
{
    type Future = U::Future;

    fn check(&mut self, request: &T) -> Self::Future {
        self(request).into_future()
    }
}
