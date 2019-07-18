use crate::error;
use futures::{future, stream, Async, Future, Poll, Stream};
pub use indexmap::Equivalent;
use indexmap::IndexMap;
use log::{debug, trace};
use std::hash::Hash;
use tokio_sync::oneshot;
use tower_service::Service;
use tower_util::Ready;

/// Caches services, segregating ready and unready services.
#[derive(Debug)]
pub struct ReadyCache<K, S, Req>
where
    K: Eq + Hash,
{
    next_ready_index: Option<usize>,
    ready: IndexMap<K, S>,
    pending: stream::FuturesUnordered<Pending<K, S, Req>>,
    cancelations: IndexMap<K, oneshot::Sender<()>>,
}

#[derive(Debug)]
enum PendingError<K, E> {
    Canceled(K),
    Inner(K, E),
}

/// A Future that becomes satisfied when an `S`-typed service is ready.
///
/// May fail due to cancelation, i.e. if the service is removed from discovery.
#[derive(Debug)]
struct Pending<K, S, Req> {
    key: Option<K>,
    cancel: oneshot::Receiver<()>,
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
            next_ready_index: None,
            ready: IndexMap::default(),
            cancelations: IndexMap::default(),
            pending: stream::FuturesUnordered::new(),
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
        self.cancelations.contains_key(key)
    }

    /// Obtains a reference to a service in the ready set by key.
    pub fn get_ready<Q: Hash + Equivalent<K>>(&self, key: &Q) -> Option<(usize, &K, &S)> {
        self.ready.get_full(key)
    }

    /// Obtains a mutable reference to a service in the ready set by key.
    pub fn get_ready_mut<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Option<(usize, &K, &mut S)> {
        self.ready.get_full_mut(key)
    }

    /// Obtains a reference to a service in the ready set by index.
    pub fn get_ready_index(&self, idx: usize) -> Option<(&K, &S)> {
        self.ready.get_index(idx)
    }

    /// Obtains a mutable reference to a service in the ready set by index.
    pub fn get_ready_index_mut(&mut self, idx: usize) -> Option<(&mut K, &mut S)> {
        self.ready.get_index_mut(idx)
    }

    /// Evicts an item from the cache.
    pub fn evict<Q: Hash + Equivalent<K>>(&mut self, key: &Q) -> bool {
        let canceled = if let Some(c) = self.cancelations.swap_remove(key) {
            c.send(()).expect("cancel receiver lost");
            true
        } else {
            false
        };

        match self.ready.swap_remove_full(key) {
            None => canceled,
            Some((idx, _, _)) => {
                self.repair_next_ready_index(idx);
                true
            }
        }
    }

    // Updates `next_ready_index_idx` after the entry at `rm_idx` was evicted.
    fn repair_next_ready_index(&mut self, rm_idx: usize) {
        self.next_ready_index = self.next_ready_index.take().and_then(|idx| {
            let new_sz = self.ready.len();
            debug_assert!(idx <= new_sz && rm_idx <= new_sz);
            let repaired = match idx {
                i if i == rm_idx => None,         // removed
                i if i == new_sz => Some(rm_idx), // swapped
                i => Some(i),                     // uneffected
            };
            trace!(
                "repair_index: orig={}; rm={}; sz={}; => {:?}",
                idx,
                rm_idx,
                new_sz,
                repaired,
            );
            repaired
        });
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Clone + Eq + Hash,
    S: Service<Req>,
    <S as Service<Req>>::Error: Into<error::Error>,
    S::Error: Into<error::Error>,
{
    /// Pushes a new service onto the unready set.
    ///
    /// The service will be promoted to the ready set as `poll_pending` is invoked.
    ///
    /// Note that this does not replace a service currently in the ready set with
    /// an equivalent key until this service becomes ready or the existing
    /// service becomes unready.
    pub fn push_service(&mut self, key: K, svc: S) {
        let (tx, cancel) = oneshot::channel();
        if let Some(c) = self.cancelations.insert(key.clone(), tx) {
            // If there is already a pending service for this key, cancel it.
            c.send(()).expect("cancel receiver lost");
        }
        self.pending.push(Pending {
            key: Some(key),
            ready: Ready::new(svc),
            cancel,
        });
    }

    /// Polls unready services to readiness.
    ///
    /// Returns `Async::Ready` when there are no remaining unready services.
    /// `poll_ready` should be called again after `push_service` or
    /// `call_ready_index` are invoked.
    pub fn poll_pending(&mut self) -> Poll<(), error::Failed<K>> {
        loop {
            match self.pending.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::Ready(Some((key, svc)))) => {
                    trace!("endpoint ready");
                    self.cancelations.remove(&key).expect("cancel sender lost");
                    self.ready.insert(key, svc);
                }
                Err(PendingError::Canceled(_)) => {
                    debug!("endpoint canceled");
                    // Either the cancelation has already been removed, or a
                    // replacement unready service has already been pushed.
                }
                Err(PendingError::Inner(key, e)) => {
                    self.cancelations
                        .swap_remove(&key)
                        .expect("missing cancelation");
                    return Err(error::Failed(key, e.into()));
                }
            }
        }
    }

    /// Configures the ready service to be used by `call`.
    ///
    /// This may be called instead of invoking `Service::poll_ready`
    pub fn poll_next_ready<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Poll<(), error::Failed<K>> {
        match self.ready.get_full_mut(key) {
            None => Ok(Async::NotReady),
            Some((index, _, _)) => self.poll_next_ready_index(index),
        }
    }

    /// Consume the `next_ready_index`.
    pub fn take_next_ready_index(&mut self) -> Option<usize> {
        self.next_ready_index.take()
    }

    /// Configures the ready service to be used by `call`.
    ///
    /// This may be called instead of `Service::poll_ready`.
    ///
    /// If the service is no longer ready and there are no equivalent unready
    /// services, it is moved back into the ready set and `Async::NotReady` is
    /// returned.
    ///
    /// If the service errors, it is removed and dropped and the error is returned.
    ///
    /// Otherwise, the `next_ready_index` is set with the provided index and
    /// `Async::Ready` is returned.
    pub fn poll_next_ready_index(&mut self, index: usize) -> Poll<(), error::Failed<K>> {
        self.next_ready_index = None;

        let (_, svc) = self.ready.get_index_mut(index).expect("index out of range");
        match svc.poll_ready() {
            Ok(Async::Ready(())) => {
                self.next_ready_index = Some(index);
                Ok(Async::Ready(()))
            }
            Ok(Async::NotReady) => {
                // became unready; so move it back there.
                let (key, svc) = self
                    .ready
                    .swap_remove_index(index)
                    .expect("invalid ready index");

                // If a new version of this service has been added to the
                // unready set, don't overwrite it.
                if !self.pending_contains(&key) {
                    self.push_service(key, svc);
                }

                Ok(Async::NotReady)
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

    /// Calls the next ready service.
    ///
    /// `poll_next_ready_index` must have returned Ready before this is invoked;
    /// otherwise, this method panics.
    pub fn call_next_ready(&mut self, req: Req) -> S::Future {
        let index = self
            .next_ready_index
            .take()
            .expect("poll_next_ready_index was not ready");

        let (key, mut svc) = self
            .ready
            .swap_remove_index(index)
            .expect("index out of range");

        let fut = svc.call(req);

        // If a new version of this service has been added to the
        // unready set, don't overwrite it.
        if !self.pending_contains(&key) {
            self.push_service(key, svc);
        }

        fut
    }
}

/// A default Service impleentation for ReadyCache`.
///
/// This service's `poll_ready` fails whenever an inner service fails and when
/// there are no inner services in the cache. If this behavior is not desirable,
/// users may call `poll_pending`, `poll_next_ready_index`, and `call_next_ready`
/// directly.
impl<K, S, Req> Service<Req> for ReadyCache<K, S, Req>
where
    K: Clone + Eq + Hash,
    S: Service<Req>,
    S::Error: Into<error::Error>,
    error::Failed<K>: Into<error::Error>,
{
    type Response = S::Response;
    type Error = error::Error;
    type Future = future::MapErr<S::Future, fn(S::Error) -> Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        // Drive pending endpoints to be ready. If an inner endpoint fails,
        // propagate the failure.
        self.poll_pending().map_err(Into::into)?;

        let mut ready_index = self.take_next_ready_index();
        loop {
            // If a service has already been selected, ensure that it is ready.
            // This ensures that the underlying service is ready immediately
            // before a request is dispatched to it. If, e.g., a failure
            // detector has changed the state of the service, it may be evicted
            // from the ready set so that another service can be selected.
            if let Some(index) = ready_index {
                match self.poll_next_ready_index(index) {
                    Ok(Async::NotReady) => {}
                    Ok(Async::Ready(())) => return Ok(Async::Ready(())),
                    Err(e) => return Err(e.into()),
                }
            }

            if self.ready_len() == 0 {
                if self.pending_len() == 0 {
                    // If the cache is totally exhausted, we have no means to notify the
                    return Err(error::Exhausted.into());
                }

                return Ok(Async::NotReady);
            };

            // If there are ready services use the first one in the ready set.
            // This will tend to favor more-recently added services, since the
            // last and first are swapped after each use.
            ready_index = Some(0);
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.call_next_ready(req).map_err(Into::into)
    }
}

// === Pending ===

impl<K, S, Req> Future for Pending<K, S, Req>
where
    S: Service<Req>,
{
    type Item = (K, S);
    type Error = PendingError<K, S::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.cancel.poll().expect("cancel sender lost").is_ready() {
            let key = self.key.take().expect("polled after ready");
            return Err(PendingError::Canceled(key));
        }

        match self.ready.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(svc)) => {
                let key = self.key.take().expect("polled after ready");
                Ok((key, svc).into())
            }
            Err(e) => {
                let key = self.key.take().expect("polled after ready");
                Err(PendingError::Inner(key, e))
            }
        }
    }
}
