#![deny(rust_2018_idioms)]
#![allow(elided_lifetimes_in_paths)]

//! # Tower service discovery
//!
//! Service discovery is the automatic detection of services available to the
//! consumer. These services typically live on other servers and are accessible
//! via the network; however, it is possible to discover services available in
//! other processes or even in process.

mod error;
mod list;
mod stream;

pub use crate::{list::ServiceList, stream::ServiceStream};

use futures::Poll;
use std::hash::Hash;

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
