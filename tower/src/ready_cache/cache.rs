//! A cache of services.

use super::error;
use crate::{BoxError, ServiceExt};
pub use indexmap::Equivalent;
use indexmap::IndexMap;
use std::borrow::Borrow;
use std::fmt;
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot;
use tokio::task::{JoinError, JoinMap};
use tokio_util::sync::ReusableBoxFuture;
use tokio_util::sync::ReusableBoxFuture;
use tower_service::Service;
use tracing::{debug, trace};

/// Drives readiness over a set of services.
///
/// The cache maintains two internal data structures:
///
/// * a set of _pending_ services that have not yet become ready; and
/// * a set of _ready_ services that have previously polled ready.
///
/// As each `S` typed [`Service`] is added to the cache via [`ReadyCache::push`], it
/// is added to the _pending set_. As [`ReadyCache::poll_pending`] is invoked,
/// pending services are polled and added to the _ready set_.
///
/// [`ReadyCache::call_ready`] (or [`ReadyCache::call_ready_index`]) dispatches a
/// request to the specified service, but panics if the specified service is not
/// in the ready set. The `ReadyCache::check_*` functions can be used to ensure
/// that a service is ready before dispatching a request.
///
/// The ready set can hold services for an abitrarily long time. During this
/// time, the runtime may process events that invalidate that ready state (for
/// instance, if a keepalive detects a lost connection). In such cases, callers
/// should use [`ReadyCache::check_ready`] (or [`ReadyCache::check_ready_index`])
/// immediately before dispatching a request to ensure that the service has not
/// become unavailable.
///
/// Once `ReadyCache::call_ready*` is invoked, the service is placed back into
/// the _pending_ set to be driven to readiness again.
///
/// When `ReadyCache::check_ready*` returns `false`, it indicates that the
/// specified service is _not_ ready. If an error is returned, this indicats that
/// the server failed and has been removed from the cache entirely.
///
/// [`ReadyCache::evict`] can be used to remove a service from the cache (by key),
/// though the service may not be dropped (if it is currently pending) until
/// [`ReadyCache::poll_pending`] is invoked.
///
/// Note that the by-index accessors are provided to support use cases (like
/// power-of-two-choices load balancing) where the caller does not care to keep
/// track of each service's key. Instead, it needs only to access _some_ ready
/// service. In such a case, it should be noted that calls to
/// [`ReadyCache::poll_pending`] and [`ReadyCache::evict`] may perturb the order of
/// the ready set, so any cached indexes should be discarded after such a call.
pub struct ReadyCache<K, S, Req>
where
    K: Eq + Hash,
{
    /// Tasks driving services that are not ready to readiness.
    pending: JoinMap<K, ReadyResult<S>>,

    /// Services that have previously become ready. Readiness can become stale,
    /// so a given service should be polled immediately before use.
    ready: IndexMap<K, S>,

    // XXX(eliza): bleh. it would be nice if `JoinMap` had a public
    // `poll_join_one` API...
    current_join_one: ReusableBoxFuture<'static, Option<(K, Result<ReadyResult<S>, JoinError>)>>,
    has_join_one: bool,

    _req: PhantomData<fn(Req)>,
}

type ReadyResult<S> = Result<S, BoxError>;

// Safety: This is safe because we do not use `Pin::new_unchecked`.
impl<S, K: Eq + Hash, Req> Unpin for ReadyCache<K, S, Req> {}

// === ReadyCache ===

impl<K, S, Req> Default for ReadyCache<K, S, Req>
where
    K: Eq + Hash,
    S: Service<Req>,
{
    fn default() -> Self {
        Self {
            ready: IndexMap::default(),
            pending: JoinMap::new(),
            current_join_one: ReusableBoxFuture::new(async {
                unreachable!("this should never be polled")
            }),
            has_join_one: false,
            _req: PhantomData,
        }
    }
}

impl<K, S, Req> fmt::Debug for ReadyCache<K, S, Req>
where
    K: fmt::Debug + Eq + Hash,
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Self { pending, ready, .. } = self;
        f.debug_struct("ReadyCache")
            .field("pending", pending)
            .field("ready", ready)
            .finish()
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Clone + Eq + Hash + 'static,
    S: 'static,
{
    /// Returns the total number of services in the cache.
    pub fn len(&self) -> usize {
        self.ready_len() + self.pending_len()
    }

    /// Returns whether or not there are any services in the cache.
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.pending.is_empty()
    }

    /// Returns the number of services in the ready set.
    pub fn ready_len(&self) -> usize {
        self.ready.len()
    }

    /// Returns the number of services in the unready set.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Returns true iff the given key is in the unready set.
    pub fn pending_contains<Q>(&self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        self.pending.contains_task(key)
    }

    /// Obtains a reference to a service in the ready set by key.
    pub fn get_ready<Q>(&self, key: &Q) -> Option<(usize, &K, &S)>
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        self.ready.get_full(key).map(|(i, k, v)| (i, k, &v.0))
    }

    /// Obtains a mutable reference to a service in the ready set by key.
    pub fn get_ready_mut<Q>(&mut self, key: &Q) -> Option<(usize, &K, &mut S)>
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        self.ready
            .get_full_mut(key)
            .map(|(i, k, v)| (i, k, &mut v.0))
    }

    /// Obtains a reference to a service in the ready set by index.
    pub fn get_ready_index(&self, idx: usize) -> Option<(&K, &S)> {
        self.ready.get_index(idx).map(|(k, v)| (k, &v.0))
    }

    /// Obtains a mutable reference to a service in the ready set by index.
    pub fn get_ready_index_mut(&mut self, idx: usize) -> Option<(&mut K, &mut S)> {
        self.ready.get_index_mut(idx).map(|(k, v)| (k, &mut v.0))
    }

    /// Evicts an item from the cache.
    ///
    /// Returns true if a service was marked for eviction.
    ///
    /// Services are dropped from the ready set immediately. Services in the
    /// pending set are marked for cancellation, but [`ReadyCache::poll_pending`]
    /// must be called to cause the service to be dropped.
    pub fn evict<Q>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        let canceled = self.pending.abort(key);

        self.ready
            .swap_remove_full(key)
            .map(|_| true)
            .unwrap_or(canceled)
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Eq + Hash,
    K: Clone + Send + Sync + 'static,
    S: Service<Req> + Send + Sync + 'static,
    <S as Service<Req>>::Error: Into<crate::BoxError>,
    S::Error: Into<crate::BoxError>,
{
    /// Pushes a new service onto the pending set.
    ///
    /// The service will be promoted to the ready set as [`poll_pending`] is invoked.
    ///
    /// Note that this does **not** remove services from the ready set. Once the
    /// old service is used, it will be dropped instead of being added back to
    /// the pending set; OR, when the new service becomes ready, it will replace
    /// the prior service in the ready set.
    ///
    /// [`poll_pending`]: crate::ready_cache::cache::ReadyCache::poll_pending
    pub fn push(&mut self, key: K, svc: S) {
        self.push_pending(key, svc);
    }

    fn push_pending(&mut self, key: K, svc: S) {
        self.pending.spawn(key, async move {
            svc.ready().await;
            Ok(svc)
        })
    }

    /// Polls services pending readiness, adding ready services to the ready set.
    ///
    /// Returns [`Poll::Ready`] when there are no remaining unready services.
    /// [`poll_pending`] should be called again after [`push`] or
    /// [`call_ready_index`] are invoked.
    ///
    /// Failures indicate that an individual pending service failed to become
    /// ready (and has been removed from the cache). In such a case,
    /// [`poll_pending`] should typically be called again to continue driving
    /// pending services to readiness.
    ///
    /// [`poll_pending`]: crate::ready_cache::cache::ReadyCache::poll_pending
    /// [`push`]: crate::ready_cache::cache::ReadyCache::push
    /// [`call_ready_index`]: crate::ready_cache::cache::ReadyCache::call_ready_index
    pub fn poll_pending(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), error::Failed<K>>> {
        if !self.has_join_one {
            self.current_join_one.set(self.pending.join_one());
            self.has_join_one = true;
        }

        match Pin::new(&mut self.current_join_one).poll(cx) {
            Poll::Pending => Poll::Pending,
        }
    }

    /// Checks whether the referenced endpoint is ready.
    ///
    /// Returns true if the endpoint is ready and false if it is not. An error is
    /// returned if the endpoint fails.
    pub fn check_ready<Q: Hash + Equivalent<K>>(
        &mut self,
        cx: &mut Context<'_>,
        key: &Q,
    ) -> Result<bool, error::Failed<K>> {
        match self.ready.get_full_mut(key) {
            Some((index, _, _)) => self.check_ready_index(cx, index),
            None => Ok(false),
        }
    }

    /// Checks whether the referenced endpoint is ready.
    ///
    /// If the service is no longer ready, it is moved back into the pending set
    /// and `false` is returned.
    ///
    /// If the service errors, it is removed and dropped and the error is returned.
    pub fn check_ready_index(
        &mut self,
        cx: &mut Context<'_>,
        index: usize,
    ) -> Result<bool, error::Failed<K>> {
        let svc = match self.ready.get_index_mut(index) {
            None => return Ok(false),
            Some((_, (svc, _))) => svc,
        };
        match svc.poll_ready(cx) {
            Poll::Ready(Ok(())) => Ok(true),
            Poll::Pending => {
                // became unready; so move it back there.
                let (key, (svc, cancel)) = self
                    .ready
                    .swap_remove_index(index)
                    .expect("invalid ready index");

                // If a new version of this service has been added to the
                // unready set, don't overwrite it.
                if !self.pending_contains(&key) {
                    self.push_pending(key, svc, cancel);
                }

                Ok(false)
            }
            Poll::Ready(Err(e)) => {
                // failed, so drop it.
                let (key, _) = self
                    .ready
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                Err(error::Failed(key, e.into()))
            }
        }
    }

    /// Calls a ready service by key.
    ///
    /// # Panics
    ///
    /// If the specified key does not exist in the ready
    pub fn call_ready<Q: Hash + Equivalent<K>>(&mut self, key: &Q, req: Req) -> S::Future {
        let (index, _, _) = self
            .ready
            .get_full_mut(key)
            .expect("check_ready was not called");
        self.call_ready_index(index, req)
    }

    /// Calls a ready service by index.
    ///
    /// # Panics
    ///
    /// If the specified index is out of range.
    pub fn call_ready_index(&mut self, index: usize, req: Req) -> S::Future {
        let (key, (mut svc, cancel)) = self
            .ready
            .swap_remove_index(index)
            .expect("check_ready_index was not called");

        let fut = svc.call(req);

        // If a new version of this service has been added to the
        // unready set, don't overwrite it.
        if !self.pending_contains(&key) {
            self.push_pending(key, svc, cancel);
        }

        fut
    }
}
