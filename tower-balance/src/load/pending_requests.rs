use futures::{Future, Poll, Async};
use std::marker::PhantomData;
use std::sync::Arc;
use tower_discover::{Change, Discover};
use tower_service::Service;

use Load;

pub trait Attach {
    type Message;

    fn attach(rsp: &mut Self::Message, handle: Handle);
}

/// Expresses load based on the number of currently-pending requests.
#[derive(Debug)]
pub struct PendingRequests<S, A>
where
    S: Service,
    A: Attach<Message = S::Response>,
{
    service: S,
    track: Arc<()>,
    _p: PhantomData<A>,
}

/// Wraps `inner`'s services with `PendingRequests`.
#[derive(Debug)]
pub struct WithPendingRequests<D, A>
where
    D: Discover,
    A: Attach<Message = D::Response>,
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
///
/// When the inner future completes, either with a value or an error, `pending` is
/// decremented.
#[derive(Debug)]
pub struct ResponseFuture<F, A>
where
    F: Future,
    A: Attach<Message = F::Item>,
{
    future: F,
    handle: Option<Handle>,
    _p: PhantomData<A>,
}

#[derive(Debug)]
pub struct AttachUntilResponse<M>(PhantomData<M>);

// ===== impl PendingRequests =====

impl<S, A> PendingRequests<S, A>
where
    S: Service,
    A: Attach<Message = S::Response>,
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
    A: Attach<Message = S::Response>,
{
    type Metric = Count;

    fn load(&self) -> Count {
        Count(self.count())
    }
}

impl<S, A> Service for PendingRequests<S, A>
where
    S: Service,
    A: Attach<Message = S::Response>,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, A>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.service.poll_ready()
    }

    fn call(&mut self, req: Self::Request) -> Self::Future {
        ResponseFuture {
            handle: Some(self.handle()),
            future: self.service.call(req),
            _p: PhantomData,
        }
    }
}

// ===== impl WithPendingRequests =====

impl<D, A> WithPendingRequests<D, A>
where
    D: Discover,
    A: Attach<Message = D::Response>,
{
    pub fn new(discover: D) -> Self {
        Self { discover, _p: PhantomData }
    }
}

impl<D, A> Discover for WithPendingRequests<D, A>
where
    D: Discover,
    A: Attach<Message = D::Response>,
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

// ===== impl ResponseFuture =====

impl<F, A> Future for ResponseFuture<F, A>
where
    F: Future,
    A: Attach<Message = F::Item>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.future.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => Err(e),
            Ok(Async::Ready(mut rsp)) => {
                if let Some(h) = self.handle.take() {
                    A::attach(&mut rsp, h);
                }
                Ok(rsp.into())
            }
        }
    }
}

// ===== impl AttachUntilResponse =====

impl<M> Default for AttachUntilResponse<M> {
    fn default() -> Self {
        AttachUntilResponse(PhantomData)
    }
}

impl<M> Attach for AttachUntilResponse<M> {
    type Message = M;
    fn attach(_: &mut M, _: Handle) {}
}
