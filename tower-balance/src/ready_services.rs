#![allow(dead_code, missing_docs)] // FIXME

use crate::error;
use futures::{stream, Async, Future, Poll, Stream};
pub use indexmap::Equivalent;
use indexmap::IndexMap;
use log::{debug, trace};
use std::hash::Hash;
use tokio_sync::oneshot;
use tower_service::Service;
use tower_util::Ready;

#[derive(Debug)]
pub struct ReadyServices<K, S, Req>
where
    K: Eq + Hash,
{
    ready_services: IndexMap<K, S>,
    unready_services: stream::FuturesUnordered<UnreadyService<K, S, Req>>,
    cancelations: IndexMap<K, oneshot::Sender<()>>,
}

enum Error<E> {
    Inner(E),
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

// === ReadyServices ===

impl<K, S, Req> Default for ReadyServices<K, S, Req>
where
    K: Eq + Hash,
    S: Service<Req>,
{
    fn default() -> Self {
        Self {
            ready_services: IndexMap::default(),
            cancelations: IndexMap::default(),
            unready_services: stream::FuturesUnordered::new(),
        }
    }
}

impl<K, S, Req> ReadyServices<K, S, Req>
where
    K: Eq + Hash,
{
    pub fn ready_len(&self) -> usize {
        self.ready_services.len()
    }

    pub fn unready_len(&self) -> usize {
        self.unready_services.len()
    }

    pub fn get_ready<Q: Hash + Equivalent<K>>(&self, key: &Q) -> Option<(usize, &K, &S)> {
        self.ready_services.get_full(key)
    }

    pub fn get_ready_mut<Q: Hash + Equivalent<K>>(
        &mut self,
        key: &Q,
    ) -> Option<(usize, &K, &mut S)> {
        self.ready_services.get_full_mut(key)
    }

    pub fn get_ready_index(&self, idx: usize) -> Option<(&K, &S)> {
        self.ready_services.get_index(idx)
    }

    pub fn get_ready_index_mut(&mut self, idx: usize) -> Option<(&mut K, &mut S)> {
        self.ready_services.get_index_mut(idx)
    }
}

impl<K, S, Req> ReadyServices<K, S, Req>
where
    K: Clone + Eq + Hash,
    S: Service<Req>,
    <S as Service<Req>>::Error: Into<error::Error>,
{
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

    pub fn swap_evict_ready<Q: Hash + Equivalent<K>>(&mut self, key: &Q) -> Option<usize> {
        if let Some((idx, _key, _)) = self.ready_services.swap_remove_full(key) {
            debug_assert!(!self.cancelations.contains_key(&_key));
            return Some(idx);
        }

        if let Some(cancel) = self.cancelations.remove(key) {
            let _ = cancel.send(());
        }
        None
    }

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
                Err((key, Error::Canceled)) => {
                    debug_assert!(!self.cancelations.contains_key(&key));
                }
                Err((key, Error::Inner(e))) => {
                    debug!("dropping failed endpoint: {:?}", e.into());
                    let _cancel = self.cancelations.swap_remove(&key);
                    debug_assert!(_cancel.is_some());
                }
            }
        }
    }

    pub fn poll_ready_index(&mut self, index: usize) -> Option<Poll<(), ()>> {
        let (_, svc) = self.ready_services.get_index_mut(index)?;
        match svc.poll_ready() {
            Ok(Async::Ready(())) => Some(Ok(Async::Ready(()))),
            Ok(Async::NotReady) => {
                // became unready; so move it back there.
                let (key, svc) = self
                    .ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                self.push_unready(key, svc);
                Some(Ok(Async::NotReady))
            }
            Err(e) => {
                // failed, so drop it.
                debug!("evicting failed endpoint: {:?}", e.into());
                self.ready_services
                    .swap_remove_index(index)
                    .expect("invalid ready index");
                Some(Err(()))
            }
        }
    }

    pub fn swap_process_ready_index<T, P>(&mut self, idx: usize, process: P) -> Option<T>
    where
        P: FnOnce(&mut S) -> T,
    {
        let (key, mut svc) = self.ready_services.swap_remove_index(idx)?;
        let t = (process)(&mut svc);
        self.push_unready(key, svc);
        Some(t)
    }
}

// === UnreadyService ===

impl<K, S: Service<Req>, Req> Future for UnreadyService<K, S, Req> {
    type Item = (K, S);
    type Error = (K, Error<S::Error>);

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Ok(Async::Ready(())) = self.cancel.poll() {
            let key = self.key.take().expect("polled after ready");
            return Err((key, Error::Canceled));
        }

        match self.ready.poll() {
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(svc)) => {
                let key = self.key.take().expect("polled after ready");
                Ok((key, svc).into())
            }
            Err(e) => {
                let key = self.key.take().expect("polled after ready");
                Err((key, Error::Inner(e)))
            }
        }
    }
}
