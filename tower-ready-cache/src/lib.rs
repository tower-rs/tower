use futures::{stream, Async, Future, Poll, Stream};
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
    ready_services: IndexMap<K, S>,
    unready_services: stream::FuturesUnordered<UnreadyService<K, S, Req>>,
    cancelations: IndexMap<K, oneshot::Sender<()>>,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;

enum UnreadyError {
    Inner(Error),
    Canceled,
}

/// A Future that becomes satisfied when an `S`-typed service is ready.
///
/// May fail due to cancelation, i.e. if the service is removed from discovery.
#[derive(Debug)]
struct UnreadyService<K, S, Req> {
    key: Option<K>,
    cancel: oneshot::Receiver<()>,
    ready: tower_util::Ready<S, Req>,
}

// === ReadyCache ===

impl<K, S, Req> Default for ReadyCache<K, S, Req>
where
    K: Eq + Hash,
    S: Service<Req>,
    S::Error: Into<Error>,
{
    fn default() -> Self {
        Self {
            ready_services: IndexMap::default(),
            cancelations: IndexMap::default(),
            unready_services: stream::FuturesUnordered::new(),
        }
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Eq + Hash,
{
    /// Returns the number of services in the ready set.
    pub fn ready_len(&self) -> usize {
        self.ready_services.len()
    }

    /// Returns the number of services in the unready set.
    pub fn unready_len(&self) -> usize {
        self.unready_services.len()
    }

    /// Returns true iff the given key is in the unready set.
    pub fn unready_contains<Q: Hash + Equivalent<K>>(&self, key: &Q) -> bool {
        self.cancelations.contains_key(key)
    }

    /// Attempts to cancel a service in the unready set.
    ///
    /// The service will be removed from the cache when `poll_unready` is called.
    ///
    /// Returns true iff the referenced service was unready and it was
    /// succsesfully canceled.
    pub fn cancel_unready<Q: Hash + Equivalent<K>>(&mut self, key: &Q) -> bool {
        if let Some(c) = self.cancelations.swap_remove(key) {
            c.send(()).is_ok()
        } else {
            false
        }
    }

    /// Obtains a reference to a service in the ready set by key.
    pub fn get_ready<Q: Hash + Equivalent<K>>(&self, key: &Q) -> Option<(usize, &K, &S)> {
        self.ready_services.get_full(key)
    }

    /// Obtains a mutable reference to a service in the ready set by key.
    pub fn get_ready_mut<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Option<(usize, &K, &mut S)> {
        self.ready_services.get_full_mut(key)
    }

    /// Obtains a reference to a service in the ready set by index.
    pub fn get_ready_index(&self, idx: usize) -> Option<(&K, &S)> {
        self.ready_services.get_index(idx)
    }

    /// Obtains a mutable reference to a service in the ready set by index.
    pub fn get_ready_index_mut(&mut self, idx: usize) -> Option<(&mut K, &mut S)> {
        self.ready_services.get_index_mut(idx)
    }

    /// Evicts an item from the cache.
    ///
    /// If the referenced service was ready in the ready set, its former index in
    /// the ready set is returned.
    pub fn evict<Q: Hash + Equivalent<K>>(&mut self, key: &Q) -> Option<usize> {
        self.cancel_unready(key);

        self.ready_services
            .swap_remove_full(key)
            .map(|(idx, _k, _)| {
                debug_assert!(!self.cancelations.contains_key(&_k));
                idx
            })
    }
}

impl<K, S, Req> ReadyCache<K, S, Req>
where
    K: Clone + Eq + Hash,
    S: Service<Req>,
    <S as Service<Req>>::Error: Into<Error>,
{
    /// Pushes a new service onto the unready set.
    ///
    /// The service will be promoted to the ready set as `poll_unready` is invoked.
    ///
    /// Note that this does not replace a service currently in the ready set with
    /// an equivalent key until this service becomes ready or the existing
    /// service becomes unready.
    pub fn push_unready(&mut self, key: K, svc: S) {
        let (tx, rx) = oneshot::channel();
        if let Some(c) = self.cancelations.insert(key.clone(), tx) {
            let _ = c.send(());
        }
        self.unready_services.push(UnreadyService {
            key: Some(key),
            ready: Ready::new(svc),
            cancel: rx,
        });
    }

    /// Polls unready services to readiness.
    ///
    /// Returns `Async::Ready` when there are no remaining unready services.
    /// `poll_ready` should be called again after `push_unready` or
    /// `process_ready_index` are invoked.
    pub fn poll_unready(&mut self) -> Async<()> {
        loop {
            match self.unready_services.poll() {
                Ok(Async::NotReady) => return Async::NotReady,
                Ok(Async::Ready(None)) => return Async::Ready(()),
                Ok(Async::Ready(Some((key, svc)))) => {
                    trace!("endpoint ready");
                    let _cancel = self.cancelations.remove(&key);
                    debug_assert!(_cancel.is_some(), "missing cancelation");
                    self.ready_services.insert(key, svc);
                }
                Err((key, UnreadyError::Canceled)) => {
                    debug_assert!(!self.cancelations.contains_key(&key));
                }
                Err((key, UnreadyError::Inner(e))) => {
                    debug!("dropping failed endpoint: {:?}", e);
                    let _cancel = self.cancelations.swap_remove(&key);
                    debug_assert!(_cancel.is_some());
                }
            }
        }
    }

    /// Polls a service in the ready set to determine if it is still ready.
    ///
    /// If the service is no longer ready and there are no equivalent unready
    /// services, it is moved back into the ready set and `Async::NotReady` is
    /// returned.
    ///
    /// If the service errors, it is removed and dropped and the error is returned.
    ///
    /// Otherwise, `Async::Ready` is returned.
    ///
    /// Panics if the provided `index` is out of range.
    pub fn poll_ready_index(&mut self, index: usize) -> Poll<(), Error> {
        let (_, svc) = self
            .ready_services
            .get_index_mut(index)
            .expect("index out of range");

        match svc.poll_ready() {
            Ok(Async::Ready(())) => Ok(Async::Ready(())),
            Ok(Async::NotReady) => {
                // became unready; so move it back there.
                let (key, svc) = self
                    .ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                // If a new version of this service has been added to the
                // unready set, don't overwrite it.
                if !self.unready_contains(&key) {
                    self.push_unready(key, svc);
                }
                Ok(Async::NotReady)
            }
            Err(e) => {
                // failed, so drop it.
                let e = e.into();
                debug!("evicting failed endpoint: {:?}", e);
                self.ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                Err(e)
            }
        }
    }

    /// Swap-removes the indexed service from the ready set.
    ///
    /// Panics if the provided `index` is out of range.
    pub fn process_ready_index<T, P>(&mut self, index: usize, process: P) -> T
    where
        P: FnOnce(&mut S) -> T,
    {
        let (key, mut svc) = self
            .ready_services
            .swap_remove_index(index)
            .expect("index out of range");

        let t = (process)(&mut svc);

        // If a new version of this service has been added to the
        // unready set, don't overwrite it.
        if !self.unready_contains(&key) {
            self.push_unready(key, svc);
        }

        t
    }
}

// === UnreadyService ===

impl<K, S, Req> Future for UnreadyService<K, S, Req>
where
    S: Service<Req>,
    S::Error: Into<Error>,
{
    type Item = (K, S);
    type Error = (K, UnreadyError);

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Ok(Async::Ready(())) = self.cancel.poll() {
            let key = self.key.take().expect("polled after ready");
            return Err((key, UnreadyError::Canceled));
        }

        match self.ready.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(svc)) => {
                let key = self.key.take().expect("polled after ready");
                Ok((key, svc).into())
            }
            Err(e) => {
                let key = self.key.take().expect("polled after ready");
                Err((key, UnreadyError::Inner(e.into())))
            }
        }
    }
}
