use tower_service::Service;

use std::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A boxed `Service + Send` trait object.
///
/// `BoxService` turns a service into a trait object, allowing the response
/// future type to be dynamic. This type requires both the service and the
/// response future to be `Send`.
///
/// See module level documentation for more details.
pub struct BoxService<T, U, E> {
    inner: Box<dyn Service<T, Response = U, Error = E, Future = BoxFuture<U, E>> + Send>,
}

/// A boxed `Future + Send` trait object.
///
/// This type alias represents a boxed future that is `Send` and can be moved
/// across threads.
type BoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

#[derive(Debug)]
struct Boxed<S> {
    inner: S,
}

impl<T, U, E> BoxService<T, U, E> {
    #[allow(missing_docs)]
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + Send + 'static,
        S::Future: Send + 'static,
    {
        let inner = Box::new(Boxed { inner });
        BoxService { inner }
    }
}

impl<T, U, E> Service<T> for BoxService<T, U, E> {
    type Response = U;
    type Error = E;
    type Future = BoxFuture<U, E>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: T) -> BoxFuture<U, E> {
        self.inner.call(request)
    }
}

impl<T, U, E> fmt::Debug for BoxService<T, U, E>
where
    T: fmt::Debug,
    U: fmt::Debug,
    E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxService").finish()
    }
}

impl<S, Request> Service<Request> for Boxed<S>
where
    S: Service<Request> + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        Box::pin(self.inner.call(request))
    }
}
