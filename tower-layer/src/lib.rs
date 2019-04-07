#![deny(missing_docs, rust_2018_idioms)]
#![doc(html_root_url = "https://docs.rs/tower-layer/0.1.0")]

//! Layer traits and extensions.
//!
//! A layer decorates an service and provides additional functionality. It
//! allows other services to be composed with the service that implements layer.
//!
//! A middleware implements the [`Layer`] and [`Service`] trait.

use tower_service::Service;

/// Decorates a `Service`, transforming either the request or the response.
///
/// Often, many of the pieces needed for writing network applications can be
/// reused across multiple services. The `Layer` trait can be used to write
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
/// # extern crate void;
/// # use tower_service::Service;
/// # use futures::{Poll, Async};
/// # use tower_layer::Layer;
/// # use std::fmt;
/// # use void::Void;
///
/// pub struct LogLayer {
///     target: &'static str,
/// }
///
/// impl<S, Request> Layer<S, Request> for LogLayer
/// where
///     S: Service<Request>,
///     Request: fmt::Debug,
/// {
///     type Response = S::Response;
///     type Error = S::Error;
///     type LayerError = Void;
///     type Service = LogService<S>;
///
///     fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
///         Ok(LogService {
///             target: self.target,
///             service
///         })
///     }
/// }
///
/// // This service implements the Log behavior
/// pub struct LogService<S> {
///     target: &'static str,
///     service: S,
/// }
///
/// impl<S, Request> Service<Request> for LogService<S>
/// where
///     S: Service<Request>,
///     Request: fmt::Debug,
/// {
///     type Response = S::Response;
///     type Error = S::Error;
///     type Future = S::Future;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///         self.service.poll_ready()
///     }
///
///     fn call(&mut self, request: Request) -> Self::Future {
///         // Insert log statement here or other functionality
///         println!("request = {:?}, target = {:?}", request, self.target);
///         self.service.call(request)
///     }
/// }
/// ```
///
/// The above log implementation is decoupled from the underlying protocol and
/// is also decoupled from client or server concerns. In other words, the same
/// log middleware could be used in either a client or a server.
pub trait Layer<S, Request> {
    /// The wrapped service response type
    type Response;

    /// The wrapped service's error type
    type Error;

    /// The error produced when calling `layer`
    type LayerError;

    /// The wrapped service
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;

    /// Wrap the given service with the middleware, returning a new service
    /// that has been decorated with the middleware.
    fn layer(&self, inner: S) -> Result<Self::Service, Self::LayerError>;
}
