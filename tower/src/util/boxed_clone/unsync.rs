use crate::ServiceExt;
use futures_util::future::LocalBoxFuture;
use std::{
    fmt,
    task::{Context, Poll},
};
use tower_layer::{layer_fn, LayerFn};
use tower_service::Service;

/// A boxed [`CloneService`] trait object.
///
/// This type alias represents a boxed future that is *not* [`Send`] and must
/// remain on the current thread.
pub struct UnsyncBoxCloneService<T, U, E>(
    Box<
        dyn UnsyncCloneService<T, Response = U, Error = E, Future = LocalBoxFuture<'static, Result<U, E>>>
    >,
);

impl<T, U, E> UnsyncBoxCloneService<T, U, E> {
    /// Create a new `BoxCloneService`.
    pub fn new<S>(inner: S) -> Self
    where
        S: Service<T, Response = U, Error = E> + Clone + 'static,
        S::Future: 'static,
    {
        let inner = inner.map_future(|f| Box::pin(f) as _);
        UnsyncBoxCloneService(Box::new(inner))
    }

    /// Returns a [`Layer`] for wrapping a [`Service`] in a [`BoxCloneService`]
    /// middleware.
    ///
    /// [`Layer`]: crate::Layer
    pub fn layer<S>() -> LayerFn<fn(S) -> Self>
    where
        S: Service<T, Response = U, Error = E> + Clone + 'static,
        S::Future: 'static,
    {
        layer_fn(Self::new)
    }
}

impl<T, U, E> Service<T> for UnsyncBoxCloneService<T, U, E> {
    type Response = U;
    type Error = E;
    type Future = LocalBoxFuture<'static, Result<U, E>>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), E>> {
        self.0.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, request: T) -> Self::Future {
        self.0.call(request)
    }
}

impl<T, U, E> Clone for UnsyncBoxCloneService<T, U, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

trait UnsyncCloneService<R>: Service<R> {
    fn clone_box(
        &self,
    ) -> Box<
        dyn UnsyncCloneService<R, Response = Self::Response, Error = Self::Error, Future = Self::Future>,
    >;
}

impl<R, T> UnsyncCloneService<R> for T
where
    T: Service<R> + Clone + 'static,
{
    fn clone_box(
        &self,
    ) -> Box<dyn UnsyncCloneService<R, Response = T::Response, Error = T::Error, Future = T::Future>>
    {
        Box::new(self.clone())
    }
}

impl<T, U, E> fmt::Debug for UnsyncBoxCloneService<T, U, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxCloneService").finish()
    }
}
