//! Middleware to set tokio task-local data.

use tokio::task::{futures::TaskLocalFuture, LocalKey};
use tower_layer::Layer;
use tower_service::Service;

/// A [`Layer`] that produces a [`SetTaskLocal`] service.
#[derive(Clone, Copy, Debug)]
pub struct SetTaskLocalLayer<T: 'static> {
    key: &'static LocalKey<T>,
    value: T,
}

impl<T> SetTaskLocalLayer<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new [`SetTaskLocalLayer`].
    pub const fn new(key: &'static LocalKey<T>, value: T) -> Self {
        SetTaskLocalLayer { key, value }
    }
}

impl<S, T> Layer<S> for SetTaskLocalLayer<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Service = SetTaskLocal<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        SetTaskLocal::new(inner, self.key, self.value.clone())
    }
}

/// Service returned by the [`set_task_local`] combinator.
///
/// [`set_task_local`]: crate::util::ServiceExt::set_task_local
#[derive(Clone, Copy, Debug)]
pub struct SetTaskLocal<S, T: 'static> {
    inner: S,
    key: &'static LocalKey<T>,
    value: T,
}

impl<S, T> SetTaskLocal<S, T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new [`SetTaskLocal`] service.
    pub const fn new(inner: S, key: &'static LocalKey<T>, value: T) -> Self {
        Self { inner, key, value }
    }
}

impl<S, T, R> Service<R> for SetTaskLocal<S, T>
where
    S: Service<R>,
    T: Clone + Send + Sync + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = TaskLocalFuture<T, S::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        // This is not great. I don't want to clone the value twice.
        // Probably need to introduce a custom Future that delays calling
        // inner.call until the local-key is set?
        let fut = self
            .key
            .sync_scope(self.value.clone(), || self.inner.call(req));
        self.key.scope(self.value.clone(), fut)
    }
}
