//! Middleware traits and implementations.
//!
//! A middleware decorates an service and provides additional functionality.
//! This additional functionality may include, but is not limited to:
//!
//! * Rejecting the request.
//! * Taking an action based on the request.
//! * Mutating the request before passing it along to the application.
//! * Mutating the response returned by the application.
//!
//! A middleware implements the [`Middleware`] trait.

#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower-middleware/0.1.0")]

extern crate futures;
extern crate tower_service;

#[cfg(feature = "ext")]
mod ext;

#[cfg(feature = "ext")]
pub use ext::{Chain, MiddlewareExt};

use tower_service::Service;

/// Decorates a `Service`, transforming either the request or the response.
///
/// Often, many of the pieces needed for writing network applications can be
/// reused across multiple services. The `Middleware` trait can be used to write
/// reusable components that can be applied to very different kinds of services;
/// for example, it can be applied to services operating on different protocols,
/// and to both the client and server side of a network transaction.
///
/// # Log
///
/// Take request logging as an example:
///
/// ```rust
/// # extern crate futures;
/// # extern crate tower_service;
/// # extern crate tower_web;
/// # #[macro_use] extern crate log;
/// use tower_service::Service;
/// use tower_web::middleware::Middleware;
/// use futures::{Future, Poll};
///
/// use std::fmt;
///
/// pub struct LogMiddleware {
///     target: &'static str,
/// }
///
/// impl<S> Middleware<S> for LogMiddleware
/// where
///     S: Service,
///     S::Request: fmt::Debug,
/// {
///     type Request = S::Request;
///     type Response = S::Response;
///     type Error = S::Error;
///     type Service = LogService<S>;
///
///     fn wrap(&self, service: S) -> LogService<S> {
///         LogService {
///             target: self.target,
///             service
///         }
///     }
/// }
///
/// // This service implements the Log behavior
/// pub struct LogService<S> {
///     target: &'static str,
///     service: S,
/// }
///
/// impl<S> Service for LogService<S>
/// where
///     S: Service,
///     S::Request: fmt::Debug,
/// {
///     type Request = S::Request;
///     type Response = S::Response;
///     type Error = S::Error;
///     type Future = S::Future;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///         self.service.poll_ready()
///     }
///
///     fn call(&mut self, request: Self::Request) -> Self::Future {
///         info!(target: self.target, "request = {:?}", request);
///         self.service.call(request)
///     }
/// }
/// ```
///
/// The above log implementation is decoupled from the underlying protocol and
/// is also decoupled from client or server concerns. In other words, the same
/// log middleware could be used in either a client or a server.
pub trait Middleware<S> {
    /// The wrapped service request type
    type Request;

    /// The wrapped service response type
    type Response;

    /// The wrapped service's error type
    type Error;

    /// The wrapped service
    type Service: Service<Self::Request, Response = Self::Response, Error = Self::Error>;

    /// Wrap the given service with the middleware, returning a new service
    /// that has been decorated with the middleware.
    fn wrap(&self, inner: S) -> Self::Service;
}
