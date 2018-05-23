use futures::{Async, Future, Poll};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;

pub trait Attach<H, T> {
    fn attach(handle: H, item: &mut T);

    fn attach_future<F>(handle: H, future: F) -> AttachFuture<F, H, Self>
    where
        F: Future<Item = T>,
        Self: Sized,
    {
        AttachFuture {
            future,
            handle: Some(handle),
            _p: PhantomData,
        }
    }
}

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, A = ()>
where
    S: Service,
    A: Attach<Handle, S::Response>,
{
    service: S,
    track: Arc<()>,
    _p: PhantomData<A>,
}

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, A = ()>
where
    D: Discover,
    A: Attach<Handle, D::Response>,
{
    discover: D,
    _p: PhantomData<A>,
}

/// Represents the number of currently-pending requests to a given service.
#[derive(Clone, Copy, Debug, Default, PartialOrd, PartialEq, Ord, Eq)]
pub struct Count(usize);

/// Ensures that `pending` is decremented.
#[derive(Debug)]
pub struct Handle(Arc<()>);

/// Wraps a response future so that it is tracked by `PendingRequests`.
#[derive(Debug)]
pub struct AttachFuture<F, H, A = ()>
where
    F: Future,
    A: Attach<H, F::Item>,
{
    future: F,
    handle: Option<H>,
    _p: PhantomData<A>,
}

// ===== impl PendingRequests =====

impl<S, A> PendingRequests<S, A>
where
    S: Service,
    A: Attach<Handle, S::Response>,
{
    pub fn new(service: S) -> Self {
        Self {
            service,
            track: Arc::new(()),
            _p: PhantomData,
        }
    }

    pub fn count(&self) -> usize {
        // Count the number of references that aren't `self.track`.
        Arc::strong_count(&self.track) - 1
    }

    fn handle(&self) -> Handle {
        Handle(self.track.clone())
    }
}

impl<S, A> Load for PendingRequests<S, A>
where
    S: Service,
    A: Attach<Handle, S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.count())
    }
}

impl<S, A> Service for PendingRequests<S, A>
where
    S: Service,
    A: Attach<Handle, S::Response>,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = AttachFuture<S::Future, Handle, A>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        A::attach_future(self.handle(), self.service.call(req))
    }
}

// ===== impl WithPendingRequests =====

impl<D, A> WithPendingRequests<D, A>
where
    D: Discover,
    A: Attach<Handle, D::Response>,
{
    pub fn new(discover: D) -> Self {
        Self {
            discover,
            _p: PhantomData,
        }
    }
}

impl<D, A> Discover for WithPendingRequests<D, A>
where
    D: Discover,
    A: Attach<Handle, D::Response>,
{
    type Key = D::Key;
    type Request = D::Request;
    type Response = D::Response;
    type Error = D::Error;
    type Service = PendingRequests<D::Service, A>;
    type DiscoverError = D::DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<D::Key, Self::Service>, D::DiscoverError> {
        use self::Change::*;

        let change = match try_ready!(self.discover.poll()) {
            Insert(k, svc) => Insert(k, PendingRequests::new(svc)),
            Remove(k) => Remove(k),
        };

        Ok(Async::Ready(change))
    }
}

// ===== impl AttachFuture =====

impl<F, H, A> Future for AttachFuture<F, H, A>
where
    F: Future,
    A: Attach<H, F::Item>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.future.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => Err(e),
            Ok(Async::Ready(mut item)) => {
                if let Some(h) = self.handle.take() {
                    A::attach(h, &mut item);
                }
                Ok(item.into())
            }
        }
    }
}

// ===== impl AttachUntilResponse =====

impl<H, M> Attach<H, M> for () {
    fn attach(_: H, _: &mut M) {}
}
