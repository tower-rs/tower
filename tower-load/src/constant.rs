//! A constant `Load` implementation. Primarily useful for testing.

use std::task::{Context, Poll};
use tower_discover::{Change, Discover};
use tower_service::Service;

use crate::Load;

/// Wraps a type so that `Load::load` returns a constant value.
pub struct Constant<T, M> {
    inner: T,
    load: M,
}

// ===== impl Constant =====

impl<T, M: Copy> Constant<T, M> {
    /// Wraps a `T`-typed service with a constant `M`-typed load metric.
    pub fn new(inner: T, load: M) -> Self {
        Self { inner, load }
    }
}

impl<T, M: Copy + PartialOrd> Load for Constant<T, M> {
    type Metric = M;

    fn load(&self) -> M {
        self.load
    }
}

impl<S, M, Request> Service<Request> for Constant<S, M>
where
    S: Service<Request>,
    M: Copy,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        self.inner.call(req)
    }
}

/// Proxies `Discover` such that all changes are wrapped with a constant load.
impl<D: Discover, M: Copy> Discover for Constant<D, M> {
    type Key = D::Key;
    type Service = Constant<D::Service, M>;
    type Error = D::Error;

    /// Yields the next discovery change set.
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<D::Key, Self::Service>, D::Error>> {
        use self::Change::*;

        let change = match self.inner.poll(cx) {
            Poll::Ready(Ok(Insert(k, svc))) => Insert(k, Constant::new(svc, self.load)),
            Poll::Ready(Ok(Remove(k))) => Remove(k),
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        };

        Poll::Ready(Ok(change))
    }
}

impl<D, M> Unpin for Constant<D, M> {}
