use tower_layer::{layer_fn, LayerFn};
use tower_service::Service;

use std::any::Any;
use std::fmt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A boxed [`Service`] trait object.
pub struct UnsyncBoxService<T, U, E> {
    inner: Box<
        dyn Service<
            T,
            Response = U,
            Error = E,
            Token = Box<dyn Any>,
            Future = UnsyncBoxFuture<U, E>,
        >,
    >,
}

/// A boxed [`Future`] trait object.
///
/// This type alias represents a boxed future that is *not* [`Send`] and must
/// remain on the current thread.
type UnsyncBoxFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;

#[derive(Debug)]
struct UnsyncBoxed<S> {
    inner: S,
}

impl<T, U, E> UnsyncBoxService<T, U, E> {
    #[allow(missing_docs)]
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + 'static,
        S::Token: Any + 'static,
        S::Future: 'static,
    {
        let inner = Box::new(UnsyncBoxed { inner });
        UnsyncBoxService { inner }
    }

    /// Returns a [`Layer`] for wrapping a [`Service`] in an [`UnsyncBoxService`] middleware.
    ///
    /// [`Layer`]: crate::Layer
    pub fn layer<S>() -> LayerFn<fn(S) -> Self>
    where
        S: Service<T, Response = U, Error = E> + 'static,
        S::Token: Any + 'static,
        S::Future: 'static,
    {
        layer_fn(Self::new)
    }
}

impl<T, U, E> Service<T> for UnsyncBoxService<T, U, E> {
    type Response = U;
    type Error = E;
    type Token = Box<dyn Any>;
    type Future = UnsyncBoxFuture<U, E>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, E>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, token: Self::Token, request: T) -> UnsyncBoxFuture<U, E> {
        self.inner.call(token, request)
    }
}

impl<T, U, E> fmt::Debug for UnsyncBoxService<T, U, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("UnsyncBoxService").finish()
    }
}

impl<S, Request> Service<Request> for UnsyncBoxed<S>
where
    S: Service<Request> + 'static,
    S::Token: Any + 'static,
    S::Future: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Token = Box<dyn Any>;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, Self::Error>> {
        self.inner
            .poll_ready(cx)
            .map_ok(|t| Box::new(t) as Box<dyn Any>)
    }

    fn call(&mut self, token: Self::Token, request: Request) -> Self::Future {
        Box::pin(
            self.inner.call(
                *token
                    .downcast()
                    .expect("Invalid token passed to UnsyncBoxService."),
                request,
            ),
        )
    }
}
