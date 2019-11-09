use crate::error;
use futures::{stream, Async, Future, Poll, Stream};
pub use indexmap::Equivalent;
use indexmap::IndexMap;
use log::{debug, trace};
use std::hash::Hash;
use tokio_sync::oneshot;
use tower_service::Service;

/// Caches services, providing access to services that are ready.
#[derive(Debug)]
pub struct ReadyCache<K, S, Req>
where
    K: Eq + Hash,
{
    /// A stream of services that are not yet ready.
    pending: stream::FuturesUnordered<Pending<K, S, Req>>,
    /// An index of cancelation handles for pending streams.
    pending_cancel_txs: IndexMap<K, CancelTx>,

    /// Services that have previously become ready. Readiness can become stale,
    /// so a given service should be polled immediately before use.
    ///
    /// The cancelation oneshot is preserved (though unused) while the service is
    /// ready so that it need not be reallocated each time a request is
    /// dispatched.
    ready: IndexMap<K, (S, CancelPair)>,
}

type CancelRx = oneshot::Receiver<()>;
type CancelTx = oneshot::Sender<()>;
type CancelPair = (CancelTx, CancelRx);

#[derive(Debug)]
enum PendingError<K, E> {
    Canceled(K),
    Inner(K, E),
}

/// A Future that becomes satisfied when an `S`-typed service is ready.
///
/// May fail due to cancelation, i.e. if the service is evicted from the balancer.
#[derive(Debug)]
struct Pending<K, S, Req> {
    key: Option<K>,
    cancel: Option<CancelRx>,
    ready: tower_util::Ready<S, Req>,
}

// === ReadyCache ===

impl<K, S, Req> Default for ReadyCache<K, S, Req>
where
    K: Eq + Hash,
    S: Service<Req>,
{
    fn default() -> Self {
        Self {
            ready: IndexMap::default(),
            pending: stream::FuturesUnordered::new(),
            pending_cancel_txs: IndexMap::default(),
        }
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Eq + Hash,
{
    /// Returns the total number of services in the cache.
    pub fn len(&self) -> usize {
        self.ready_len() + self.pending_len()
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
    pub fn pending_contains<Q: Hash + Equivalent<K>>(&self, key: &Q) -> bool {
        self.pending_cancel_txs.contains_key(key)
    }

    /// Obtains a reference to a service in the ready set by key.
    pub fn get_ready<Q: Hash + Equivalent<K>>(&self, key: &Q) -> Option<(usize, &K, &S)> {
        self.ready.get_full(key).map(|(i, k, v)| (i, k, &v.0))
    }

    /// Obtains a mutable reference to a service in the ready set by key.
    pub fn get_ready_mut<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Option<(usize, &K, &mut S)> {
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
    /// Returns true if a service was marked for eviction. The inner service may
    /// be retained until `poll_pending` is called.
    pub fn evict<Q: Hash + Equivalent<K>>(&mut self, key: &Q) -> bool {
        let canceled = if let Some(c) = self.pending_cancel_txs.swap_remove(key) {
            c.send(()).expect("cancel receiver lost");
            true
        } else {
            false
        };

        self.ready
            .swap_remove_full(key)
            .map(|_| true)
            .unwrap_or(canceled)
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Clone + Eq + Hash,
    S: Service<Req>,
    <S as Service<Req>>::Error: Into<error::Error>,
    S::Error: Into<error::Error>,
{
    /// Pushes a new service onto the pending set.
    ///
    /// The service will be promoted to the ready set as `poll_pending` is invoked.
    ///
    /// Note that this does not replace a service currently in the ready set with
    /// an equivalent key until this service becomes ready or the existing
    /// service becomes unready.
    pub fn push(&mut self, key: K, svc: S) {
        let cancel = oneshot::channel();
        self.push_pending(key, svc, cancel);
    }

    fn push_pending(&mut self, key: K, svc: S, (cancel_tx, cancel_rx): CancelPair) {
        if let Some(c) = self.pending_cancel_txs.insert(key.clone(), cancel_tx) {
            // If there is already a service for this key, cancel it.
            c.send(()).expect("cancel receiver lost");
        }
        self.pending.push(Pending {
            key: Some(key),
            cancel: Some(cancel_rx),
            ready: tower_util::Ready::new(svc),
        });
    }

    /// Polls services pending readiness, adding ready services to the ready set.
    ///
    /// Returns `Async::Ready` when there are no remaining unready services.
    /// `poll_pending` should be called again after `push_service` or
    /// `call_ready_index` are invoked.
    pub fn poll_pending(&mut self) -> Poll<(), error::Failed<K>> {
        loop {
            match self.pending.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::Ready(Some((key, svc, cancel_rx)))) => {
                    trace!("endpoint ready");
                    let cancel_tx = self
                        .pending_cancel_txs
                        .swap_remove(&key)
                        .expect("missing cancelation");
                    // Keep track of the cancelation so that it need not be
                    // recreated after the service is used.
                    self.ready.insert(key, (svc, (cancel_tx, cancel_rx)));
                }
                Err(PendingError::Canceled(_)) => {
                    debug!("endpoint canceled");
                    // Either the cancelation has already been removed, or a
                    // replacement unready service has already been pushed.
                }
                Err(PendingError::Inner(key, e)) => {
                    self.pending_cancel_txs
                        .swap_remove(&key)
                        .expect("missing cancelation");
                    return Err(error::Failed(key, e.into()));
                }
            }
        }
    }

    /// Checks whether the referenced endpoint is ready.
    ///
    /// Returns true if the endpoint is ready and false if it is not. An error is
    /// returned if the endpoint fails.
    pub fn check_ready<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Result<bool, error::Failed<K>> {
        match self.ready.get_full_mut(key) {
            Some((index, _, _)) => self.check_ready_index(index),
            None => Ok(false),
        }
    }

    /// Checks whether the referenced endpoint is ready.
    ///
    /// If the service is no longer ready and there are no equivalent unready
    /// services, it is moved back into the ready set and `Async::NotReady` is
    /// returned.
    ///
    /// If the service errors, it is removed and dropped and the error is returned.
    ///
    /// Otherwise, the `next_ready_index` is set with the provided index and
    /// `Async::Ready` is returned.
    pub fn check_ready_index(&mut self, index: usize) -> Result<bool, error::Failed<K>> {
        let svc = match self.ready.get_index_mut(index) {
            None => return Ok(false),
            Some((_, (svc, _))) => svc,
        };
        match svc.poll_ready() {
            Ok(Async::Ready(())) => Ok(true),
            Ok(Async::NotReady) => {
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
            Err(e) => {
                // failed, so drop it.
                let (key, _) = self
                    .ready
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                Err(error::Failed(key, e.into()))
            }
        }
    }

    /// Calls a ready service by key
    ///
    /// Panics if `check_ready` did not return true in the same task invocation
    /// as this call.
    pub fn call_ready<Q: Hash + Equivalent<K>>(&mut self, key: &Q, req: Req) -> S::Future {
        let (index, _, _) = self
            .ready
            .get_full_mut(key)
            .expect("check_ready was not called");
        self.call_ready_index(index, req)
    }

    /// Calls a ready service by index
    ///
    /// Panics if `check_ready_index` did not return true in the same task
    /// invocation as this call.
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

// === Pending ===

impl<K, S, Req> Future for Pending<K, S, Req>
where
    S: Service<Req>,
{
    type Item = (K, S, CancelRx);
    type Error = PendingError<K, S::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self
            .cancel
            .as_mut()
            .expect("polled after complete")
            .poll()
            .expect("cancel sender lost")
            .is_ready()
        {
            let key = self.key.take().expect("polled after complete");
            return Err(PendingError::Canceled(key));
        }

        match self.ready.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(svc)) => {
                let key = self.key.take().expect("polled after complete");
                let cancel = self.cancel.take().expect("polled after complete");
                Ok((key, svc, cancel).into())
            }
            Err(e) => {
                let key = self.key.take().expect("polled after compete");
                Err(PendingError::Inner(key, e))
            }
        }
    }
}
