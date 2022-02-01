//! Middleware for shedding load when inner services aren't ready.

use std::task::{Context, Poll};
use tower_service::Service;

pub mod error;
pub mod future;
mod layer;

use self::future::ResponseFuture;
pub use self::layer::LoadShedLayer;

/// A [`Service`] that sheds load when the inner service isn't ready.
///
/// [`Service`]: crate::Service
#[derive(Debug)]
pub struct LoadShed<S> {
    inner: S,
}

// ===== impl LoadShed =====

impl<S> LoadShed<S> {
    /// Wraps a service in [`LoadShed`] middleware.
    pub fn new(inner: S) -> Self {
        LoadShed { inner }
    }
}

#[derive(Debug)]
pub struct Token<T>(Option<T>);

impl<S, Req> Service<Req> for LoadShed<S>
where
    S: Service<Req>,
    S::Error: Into<crate::BoxError>,
{
    type Response = S::Response;
    type Error = crate::BoxError;
    type Token = Token<S::Token>;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Token, Self::Error>> {
        // We check for readiness here, so that we can know in `call` if
        // the inner service is overloaded or not.
        let token = match self.inner.poll_ready(cx) {
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
            Poll::Ready(Ok(token)) => Some(token),
            Poll::Pending => None,
        };

        // But we always report Ready, so that layers above don't wait until
        // the inner service is ready (the entire point of this layer!)
        Poll::Ready(Ok(Token(token)))
    }

    fn call(&mut self, token: Self::Token, req: Req) -> Self::Future {
        if let Some(token) = token.0 {
            ResponseFuture::called(self.inner.call(token, req))
        } else {
            ResponseFuture::overloaded()
        }
    }
}

impl<S: Clone> Clone for LoadShed<S> {
    fn clone(&self) -> Self {
        LoadShed {
            inner: self.inner.clone(),
        }
    }
}
