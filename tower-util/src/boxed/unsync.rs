use futures::{Future, Poll};
use tower_service::Service;

use std::fmt;

/// A boxed `Service` trait object.
pub struct UnsyncBoxService<T, U, E> {
    inner: Box<dyn Service<T, Response = U, Error = E, Future = UnsyncBoxFuture<U, E>>>,
}

/// A boxed `Future` trait object.
///
/// This type alias represents a boxed future that is *not* `Send` and must
/// remain on the current thread.
type UnsyncBoxFuture<T, E> = Box<dyn Future<Item = T, Error = E>>;

#[derive(Debug)]
struct UnsyncBoxed<S> {
    inner: S,
}

impl<T, U, E> UnsyncBoxService<T, U, E> {
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + 'static,
        S::Future: 'static,
    {
        let inner = Box::new(UnsyncBoxed { inner });
        UnsyncBoxService { inner }
    }
}

impl<T, U, E> Service<T> for UnsyncBoxService<T, U, E> {
    type Response = U;
    type Error = E;
    type Future = UnsyncBoxFuture<U, E>;

    fn poll_ready(&mut self) -> Poll<(), E> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: T) -> UnsyncBoxFuture<U, E> {
        self.inner.call(request)
    }
}

impl<T, U, E> fmt::Debug for UnsyncBoxService<T, U, E>
where
    T: fmt::Debug,
    U: fmt::Debug,
    E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnsyncBoxService").finish()
    }
}

impl<S, Request> Service<Request> for UnsyncBoxed<S>
where
    S: Service<Request> + 'static,
    S::Future: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Box<dyn Future<Item = S::Response, Error = S::Error>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        Box::new(self.inner.call(request))
    }
}
