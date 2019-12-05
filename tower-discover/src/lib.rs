#![doc(html_root_url = "https://docs.rs/tower-discover/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
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

use std::hash::Hash;
use std::ops;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Provide a uniform set of services able to satisfy a request.
///
/// This set of services may be updated over time. On each change to the set, a
/// new `NewServiceSet` is yielded by `Discover`.
///
/// See crate documentation for more details.
pub trait Discover {
    /// NewService key
    type Key: Hash + Eq;

    /// The type of `Service` yielded by this `Discover`.
    type Service;

    /// Error produced during discovery
    type Error;

    /// Yields the next discovery change set.
    fn poll_discover(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>>;
}

// delegate through Pin
impl<P> Discover for Pin<P>
where
    P: Unpin + ops::DerefMut,
    P::Target: Discover,
{
    type Key = <<P as ops::Deref>::Target as Discover>::Key;
    type Service = <<P as ops::Deref>::Target as Discover>::Service;
    type Error = <<P as ops::Deref>::Target as Discover>::Error;

    fn poll_discover(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>> {
        Pin::get_mut(self).as_mut().poll_discover(cx)
    }
}
impl<D: ?Sized + Discover + Unpin> Discover for &mut D {
    type Key = D::Key;
    type Service = D::Service;
    type Error = D::Error;

    fn poll_discover(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>> {
        Discover::poll_discover(Pin::new(&mut **self), cx)
    }
}

impl<D: ?Sized + Discover + Unpin> Discover for Box<D> {
    type Key = D::Key;
    type Service = D::Service;
    type Error = D::Error;

    fn poll_discover(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Change<Self::Key, Self::Service>, Self::Error>> {
        D::poll_discover(Pin::new(&mut *self), cx)
    }
}

/// A change in the service set
#[derive(Debug)]
pub enum Change<K, V> {
    /// A new service identified by key `K` was identified.
    Insert(K, V),
    /// The service identified by key `K` disappeared.
    Remove(K),
}
