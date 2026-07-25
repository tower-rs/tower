use tower_layer::{layer_fn, LayerFn};
use tower_service::Service;

use std::fmt;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

/// A boxed [`Service`] trait object.
pub struct UnsyncBoxService<'a, T, U, E> {
    inner: Box<dyn Service<T, Response = U, Error = E, Future = UnsyncBoxFuture<'a, U, E>> + 'a>,
}

/// A boxed [`Future`] trait object.
///
/// This type alias represents a boxed future that is *not* [`Send`] and must
/// remain on the current thread.
type UnsyncBoxFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + 'a>>;

#[derive(Debug)]
struct UnsyncBoxed<'a, S> {
    inner: S,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T, U, E> UnsyncBoxService<'a, T, U, E> {
    #[allow(missing_docs)]
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + 'a,
        S::Future: 'a,
    {
        let inner = Box::new(UnsyncBoxed {
            inner,
            _marker: PhantomData,
        });
        UnsyncBoxService { inner }
    }

    /// Returns a [`Layer`] for wrapping a [`Service`] in an [`UnsyncBoxService`] middleware.
    ///
    /// [`Layer`]: crate::Layer
    pub fn layer<S>() -> LayerFn<fn(S) -> Self>
    where
        S: Service<T, Response = U, Error = E> + 'a,
        S::Future: 'a,
    {
        layer_fn(Self::new)
    }
}

impl<'a, T, U, E> Service<T> for UnsyncBoxService<'a, T, U, E> {
    type Response = U;
    type Error = E;
    type Future = UnsyncBoxFuture<'a, U, E>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: T) -> UnsyncBoxFuture<'a, U, E> {
        self.inner.call(request)
    }
}

impl<'a, T, U, E> fmt::Debug for UnsyncBoxService<'a, T, U, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("UnsyncBoxService").finish()
    }
}

impl<'a, S, Request> Service<Request> for UnsyncBoxed<'a, S>
where
    S: Service<Request> + 'a,
    S::Future: 'a,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + 'a>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        Box::pin(self.inner.call(request))
    }
}
