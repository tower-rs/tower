//! Trait object `Service` instances
//!
//! Dynamically dispatched `Service` objects allow for erasing the underlying
//! `Service` type and using the `Service` instances as opaque handles. This can
//! be useful when the service instance cannot be explicitly named for whatever
//! reason.
//!
//! There are two variants of service objects. `BoxService` requires both the
//! service and the response future to be `Send`. These values can move freely
//! across threads. `UnsyncBoxService` requires both the service and the
//! response future to remain on the current thread. This is useful for
//! representing services that are backed by `Rc` or other non-`Send` types.
//!
//! # Examples
//!
//! ```
//! # extern crate futures;
//! # extern crate tower_service;
//! # extern crate tower_util;
//! # use futures::*;
//! # use futures::future::FutureResult;
//! # use tower_service::*;
//! # use tower_util::*;
//! // Respond to requests using a closure. Since closures cannot be named,
//! // `ServiceFn` cannot be named either
//! pub struct ServiceFn<F> {
//!     f: F,
//! }
//!
//! impl<F> Service for ServiceFn<F>
//! where F: Fn(String) -> String,
//! {
//!     type Request = String;
//!     type Response = String;
//!     type Error = ();
//!     type Future = FutureResult<String, ()>;
//!
//!     fn poll_ready(&mut self) -> Poll<(), ()> {
//!         Ok(().into())
//!     }
//!
//!     fn call(&mut self, request: String) -> FutureResult<String, ()> {
//!         future::ok((self.f)(request))
//!     }
//! }
//!
//! pub fn main() {
//!     let f = |mut request: String| {
//!         request.push_str(" response");
//!         request
//!     };
//!
//!     let service: BoxService<String, String, ()> =
//!         BoxService::new(ServiceFn { f });
//! # drop(service);
//! }
//! ```

use futures::{Future, Poll};
use tower_service::Service;

use std::fmt;

/// A boxed `Service + Send` trait object.
///
/// `BoxService` turns a service into a trait object, allowing the response
/// future type to be dynamic. This type requires both the service and the
/// response future to be `Send`.
///
/// See module level documentation for more details.
pub struct BoxService<T, U, E> {
    inner: Box<Service<T,
                      Response = U,
                         Error = E,
                        Future = BoxFuture<U, E>> + Send>,
}

/// A boxed `Future + Send` trait object.
///
/// This type alias represents a boxed future that is `Send` and can be moved
/// across threads.
pub type BoxFuture<T, E> = Box<Future<Item = T, Error = E> + Send>;

/// A boxed `Service` trait object.
pub struct UnsyncBoxService<T, U, E> {
    inner: Box<Service<T,
                      Response = U,
                         Error = E,
                        Future = UnsyncBoxFuture<U, E>>>,
}

/// A boxed `Future` trait object.
///
/// This type alias represents a boxed future that is *not* `Send` and must
/// remain on the current thread.
pub type UnsyncBoxFuture<T, E> = Box<Future<Item = T, Error = E>>;

#[derive(Debug)]
struct Boxed<S> {
    inner: S,
}

#[derive(Debug)]
struct UnsyncBoxed<S> {
    inner: S,
}

// ===== impl BoxService =====

impl<T, U, E> BoxService<T, U, E>
{
    pub fn new<S>(inner: S) -> Self
        where S: Service<T, Response = U, Error = E> + Send + 'static,
              S::Future: Send + 'static,
    {
        let inner = Box::new(Boxed { inner });
        BoxService { inner }
    }
}

impl<T, U, E> Service<T> for BoxService<T, U, E> {
    type Response = U;
    type Error = E;
    type Future = BoxFuture<U, E>;

    fn poll_ready(&mut self) -> Poll<(), E> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: T) -> BoxFuture<U, E> {
        self.inner.call(request)
    }
}

impl<T, U, E> fmt::Debug for BoxService<T, U, E>
where T: fmt::Debug,
      U: fmt::Debug,
      E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxService")
            .finish()
    }
}

// ===== impl UnsyncBoxService =====

impl<T, U, E> UnsyncBoxService<T, U, E> {
    pub fn new<S>(inner: S) -> Self
        where S: Service<T, Response = U, Error = E> + 'static,
              S::Future: 'static,
    {
        let inner = Box::new(UnsyncBoxed { inner });
        UnsyncBoxService { inner }
    }
}

impl<T, U, E> Service<T> for UnsyncBoxService<T, U, E> {
    type Response = U;
    type Error = E;
    type Future = UnsyncBoxFuture<U, E>;

    fn poll_ready(&mut self) -> Poll<(), E> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: T) -> UnsyncBoxFuture<U, E> {
        self.inner.call(request)
    }
}

impl<T, U, E> fmt::Debug for UnsyncBoxService<T, U, E>
where T: fmt::Debug,
      U: fmt::Debug,
      E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("UnsyncBoxService")
            .finish()
    }
}

// ===== impl Boxed =====

impl<S, Request> Service<Request> for Boxed<S>
where S: Service<Request> + 'static,
      S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Box<Future<Item = S::Response,
                            Error = S::Error> + Send>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        Box::new(self.inner.call(request))
    }
}

// ===== impl UnsyncBoxed =====

impl<S, Request> Service<Request> for UnsyncBoxed<S>
where S: Service<Request> + 'static,
      S::Future: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Box<Future<Item = S::Response,
                            Error = S::Error>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, request: Request) -> Self::Future {
        Box::new(self.inner.call(request))
    }
}
