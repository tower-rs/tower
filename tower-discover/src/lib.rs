//! # Tower service discovery
//!
//! Service discovery is the automatic detection of services available to the
//! consumer. These services typically live on other servers and are accessible
//! via the network; however, it is possible to discover services available in
//! other processes or even in process.

extern crate futures;
extern crate tower_service;

use futures::{Poll, Async};
use tower_service::Service;

use std::hash::Hash;
use std::iter::{Enumerate, IntoIterator};

/// Provide a uniform set of services able to satisfy a request.
///
/// This set of services may be updated over time. On each change to the set, a
/// new `NewServiceSet` is yielded by `Discover`.
///
/// See crate documentation for more details.
pub trait Discover<Request> {
    /// NewService key
    type Key: Hash + Eq;

    /// Responses given by the discovered services
    type Response;

    /// Errors produced by the discovered services
    type Error;

    /// The discovered `Service` instance.
    type Service: Service<Request,
                         Response = Self::Response,
                            Error = Self::Error>;

    /// Error produced during discovery
    type DiscoverError;

    /// Yields the next discovery change set.
    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::DiscoverError>;
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
pub struct List<T> {
    inner: Enumerate<T>,
}

// ===== impl List =====

impl<T, U> List<T>
where
    T: Iterator<Item = U>
{
    pub fn new<I, Request>(services: I) -> List<T>
    where
        I: IntoIterator<Item = U, IntoIter = T>,
        U: Service<Request>
    {
        List { inner: services.into_iter().enumerate() }
    }
}

impl<T, U, Request> Discover<Request> for List<T>
where T: Iterator<Item = U>,
      U: Service<Request>,
{
    type Key = usize;
    type Response = U::Response;
    type Error = U::Error;
    type Service = U;
    type DiscoverError = ();

    fn poll(&mut self) -> Poll<Change<Self::Key, Self::Service>, Self::DiscoverError> {
        match self.inner.next() {
            Some((i, service)) => Ok(Change::Insert(i, service).into()),
            None => Ok(Async::NotReady),
        }
    }
}
