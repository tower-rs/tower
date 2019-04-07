#![cfg_attr(test, deny(warnings))]
#![deny(missing_debug_implementations)]
#![deny(missing_docs)]

//! tower-load-shed

use futures::Poll;
use tower_service::Service;

pub mod error;
mod future;
mod layer;

use self::error::Error;
pub use self::future::ResponseFuture;
pub use self::layer::LoadShedLayer;

/// A `Service` that sheds load when the inner service isn't ready.
#[derive(Debug)]
pub struct LoadShed<S> {
    inner: S,
    is_ready: bool,
}

// ===== impl LoadShed =====

impl<S> LoadShed<S> {
    /// Wraps a service in `LoadShed` middleware.
    pub fn new(inner: S) -> Self {
        LoadShed {
            inner,
            is_ready: false,
        }
    }
}

impl<S, Req> Service<Req> for LoadShed<S>
where
    S: Service<Req>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // We check for readiness here, so that we can know in `call` if
        // the inner service is overloaded or not.
        self.is_ready = self.inner.poll_ready().map_err(Into::into)?.is_ready();

        // But we always report Ready, so that layers above don't wait until
        // the inner service is ready (the entire point of this layer!)
        Ok(().into())
    }

    fn call(&mut self, req: Req) -> Self::Future {
        if self.is_ready {
            // readiness only counts once, you need to check again!
            self.is_ready = false;
            ResponseFuture::called(self.inner.call(req))
        } else {
            ResponseFuture::overloaded()
        }
    }
}

impl<S: Clone> Clone for LoadShed<S> {
    fn clone(&self) -> Self {
        LoadShed {
            inner: self.inner.clone(),
            // new clones shouldn't carry the readiness state, as a cloneable
            // inner service likely tracks readiness per clone.
            is_ready: false,
        }
    }
}
