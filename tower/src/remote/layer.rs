use super::Remote;
use crate::BoxError;
use std::{
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
};
use tokio::runtime::Handle;
use tower_layer::Layer;
use tower_service::Service;

/// Execute a service on a remote tokio executor.
///
/// See the module documentation for more details.
pub struct RemoteLayer<R> {
    bound: usize,
    handle: Handle,
    _p: PhantomData<fn(R)>,
}

impl<R> RemoteLayer<R> {
    /// Creates a new [`RemoteLayer`] with the provided `bound`.
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    ///
    /// The current Tokio executor is used to run the given service, which means that this method
    /// must be called while on the Tokio runtime.
    pub fn new(bound: usize) -> Self {
        Self::with_handle(bound, Handle::current())
    }

    /// Creates a new [`RemoteLayer`] with the provided `bound`, spawning onto the runtime connected
    /// to the given [`Handle`].
    ///
    /// `bound` gives the maximal number of requests that can be queued for the service before
    /// backpressure is applied to callers.
    pub fn with_handle(bound: usize, handle: Handle) -> Self {
        Self {
            bound,
            handle,
            _p: PhantomData,
        }
    }
}

impl<R> Clone for RemoteLayer<R> {
    fn clone(&self) -> Self {
        Self {
            bound: self.bound,
            handle: self.handle.clone(),
            _p: self._p,
        }
    }
}

impl<R> Debug for RemoteLayer<R> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("RemoteLayer")
            .field("bound", &self.bound)
            .field("handle", &self.handle)
            .finish()
    }
}

impl<R, S> Layer<S> for RemoteLayer<R>
where
    S: Service<R> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: 'static,
    R: Send + 'static,
    BoxError: From<S::Error>,
{
    type Service = Remote<S, R>;

    fn layer(&self, service: S) -> Self::Service {
        Remote::with_handle(service, self.bound, &self.handle)
    }
}
