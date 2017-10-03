//! # Tower service discovery
//!
//! Service discovery is the automatic detection of services available to the
//! consumer. These services typically live on other servers and are accessible
//! via the network; however, it is possible to discover services available in
//! other processes or even in process.

extern crate futures;
extern crate tower;

use futures::Stream;
use tower::{NewService, Service};

use std::mem;
use std::hash::Hash;

/// Provide a uniform set of services able to satisfy a request.
///
/// This set of services may be updated over time. On each change to the set, a
/// new `NewServiceSet` is yielded by `Discover`.
///
/// See crate documentation for more details.
pub trait Discover: Stream {
    /// Requests handled by the discovered services
    type Request;

    /// Responses given by the discovered services
    type Response;

    /// Errors produced by the discovered services
    type ServiceError;

    /// The discovered `Service` instance.
    type Service: Service<Request = Self::Request,
                         Response = Self::Response,
                            Error = Self::ServiceError>;
}

/// Uniquely identifies a `NewService` yielded by `Discover`.
pub trait Key {
    /// The unique identifier
    type Key: Hash + Eq;

    /// Return a reference to the unique identifier.
    fn key(&self) -> &Self::Key;
}

/// A set of `NewService` instances.
pub struct NewServiceSet<T> {
    entries: Vec<T>,
}

/// Immutable `NewService` iterator.
pub struct Iter<'a, T: 'a> {
    inner: ::std::slice::Iter<'a, T>,
}

// ===== impl Discover =====

impl<T, U> Discover for T
where U: NewService + Key,
      T: Stream<Item = NewServiceSet<U>>,
{
    type Request = U::Request;
    type Response = U::Response;
    type ServiceError = U::Error;
    type Service = U::Service;
}

// ===== impl NewServiceSet =====

impl<T> NewServiceSet<T>
where T: NewService + Key,
{
    /// Creates an empty `NewServiceSet`.
    pub fn new() -> Self {
        NewServiceSet {
            entries: vec![],
        }
    }

    /// Adds a value to the set.
    ///
    /// If the set already contains a value with the same key, the existing
    /// value is returned.
    pub fn insert(&mut self, value: T) -> Option<T> {
        for prev in &mut self.entries {
            if prev.key() == value.key() {
                let prev = mem::replace(prev, value);
                return Some(prev);
            }
        }

        self.entries.push(value);
        None
    }

    /// An iterator visiting all entries in the set.
    pub fn iter(&self) -> Iter<T> {
        Iter { inner: self.entries.iter() }
    }
}

// ===== impl Iter =====

impl<'a, T> Iterator for Iter<'a, T>
where T: 'a + NewService + Key
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
