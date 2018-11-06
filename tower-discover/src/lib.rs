//! # Tower service discovery
//!
//! Service discovery is the automatic detection of services available to the
//! consumer. These services typically live on other servers and are accessible
//! via the network; however, it is possible to discover services available in
//! other processes or even in process.

#[macro_use]
extern crate futures;
extern crate tower_service;

use futures::{Async, Poll, Stream};
use tower_service::Service;

use std::hash::Hash;
use std::iter::{Enumerate, IntoIterator};
use std::marker::PhantomData;

/// Provide a uniform set of services able to satisfy a request.
///
/// This set of services may be updated over time. On each change to the set, a
/// new `NewServiceSet` is yielded by `Discover`.
///
/// See crate documentation for more details.
pub trait Discover {
    /// NewService key
    type Key: Hash + Eq;

    type Service;

    /// Error produced during discovery
    type Error;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error>;
}

/// A change in the service set
pub enum Change<K, V> {
    Insert(K, V),
    Remove(K),
}

/// Static service discovery based on a predetermined list of services.
///
/// `List` is created with an initial list of services. The discovery process
/// will yield this list once and do nothing after.
pub struct List<T>
where
    T: IntoIterator,
{
    inner: Enumerate<T::IntoIter>,
}

// ===== impl List =====

impl<T, U> List<T>
where
    T: IntoIterator<Item = U>,
{
    pub fn new<Request>(services: T) -> List<T>
    where
        U: Service<Request>,
    {
        List {
            inner: services.into_iter().enumerate(),
        }
    }
}

impl<T, U> Discover for List<T>
where
    T: IntoIterator<Item = U>,
{
    type Key = usize;
    type Service = U;
    type Error = ();

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        match self.inner.next() {
            Some((i, service)) => Ok(Change::Insert(i, service).into()),
            None => Ok(Async::NotReady),
        }
    }
}

/// Dynamic service discovery based on a stream of service changes.
pub struct Services<S, K, Svc> {
    inner: futures::stream::Fuse<S>,
    _marker_k: PhantomData<K>,
    _marker_v: PhantomData<Svc>,
}

// ===== impl Services =====

impl<S, K, Svc> Services<S, K, Svc>
where
    S: Stream<Item = Change<K, Svc>>,
{
    pub fn new<Request>(services: S) -> Self
    where
        Svc: Service<Request>,
    {
        Services {
            inner: services.fuse(),
            _marker_k: PhantomData,
            _marker_v: PhantomData,
        }
    }
}

impl<S, K, Svc> Discover for Services<S, K, Svc>
where
    K: Hash + Eq,
    S: Stream<Item = Change<K, Svc>>,
{
    type Key = K;
    type Service = Svc;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::Error> {
        match try_ready!(self.inner.poll()) {
            Some(c) => Ok(Async::Ready(c)),
            None => {
                // there are no more service changes coming
                Ok(Async::NotReady)
            }
        }
    }
}

// check that List can be directly over collections
#[cfg(test)]
#[allow(dead_code)]
type ListVecTest<T> = List<Vec<T>>;

#[cfg(test)]
#[allow(dead_code)]
type ListVecIterTest<T> = List<::std::vec::IntoIter<T>>;
