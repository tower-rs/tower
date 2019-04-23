#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower-service/0.2.0")]

//! Definition of the core `Service` trait to Tower
//!
//! These traits provide the necessary abstractions for defining a request /
//! response clients and servers. They are simple but powerul and are
//! used as the foundation for the rest of Tower.
//!
//! * [`Service`](trait.Service.html) is the primary trait and defines the request
//! / response exchange. See that trait for more details.

extern crate futures;

use futures::{Future, Poll};

/// An asynchronous function from `Request` to a `Response`.
///
/// The `Service` trait is a simplified interface making it easy to write
/// network applications in a modular and reusable way, decoupled from the
/// underlying protocol. It is one of Tower's fundamental abstractions.
///
/// # Functional
///
/// A `Service` is a function of a `Request`. It immediately returns a
/// `Future` representing the eventual completion of processing the
/// request. The actual request processing may happen at any time in the
/// future, on any thread or executor. The processing may depend on calling
/// other services. At some point in the future, the processing will complete,
/// and the `Future` will resolve to a response or error.
///
/// At a high level, the `Service::call` represents an RPC request. The
/// `Service` value can be a server or a client.
///
/// # Server
///
/// An RPC server *implements* the `Service` trait. Requests received by the
/// server over the network are deserialized then passed as an argument to the
/// server value. The returned response is sent back over the network.
///
/// As an example, here is how an HTTP request is processed by a server:
///
/// ```rust,ignore
/// impl Service<http::Request> for HelloWorld {
///     type Response = http::Response;
///     type Error = http::Error;
///     type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///         Ok(Async::Ready(()))
///     }
///
///     fn call(&mut self, req: http::Request) -> Self::Future {
///         // Create the HTTP response
///         let resp = http::Response::ok()
///             .with_body(b"hello world\n");
///
///         // Return the response as an immediate future
///         Box::new(futures::finished(resp))
///     }
/// }
/// ```
///
/// # Client
///
/// A client consumes a service by using a `Service` value. The client may
/// issue requests by invoking `call` and passing the request as an argument.
/// It then receives the response by waiting for the returned future.
///
/// As an example, here is how a Redis request would be issued:
///
/// ```rust,ignore
/// let client = redis::Client::new()
///     .connect("127.0.0.1:6379".parse().unwrap())
///     .unwrap();
///
/// let resp = client.call(Cmd::set("foo", "this is the value of foo"));
///
/// // Wait for the future to resolve
/// println!("Redis response: {:?}", await(resp));
/// ```
///
/// # Middleware / Layer
///
/// More often than not, all the pieces needed for writing robust, scalable
/// network applications are the same no matter the underlying protocol. By
/// unifying the API for both clients and servers in a protocol agnostic way,
/// it is possible to write middleware that provide these pieces in a
/// reusable way.
///
/// Take timeouts as an example:
///
/// ```rust,ignore
/// use tower_service::Service;
/// use tower_layer::Layer;
/// use futures::Future;
/// use std::time::Duration;
///
///
/// pub struct Timeout<T> {
///     inner: T,
///     delay: Duration,
///     timer: Timer,
/// }
///
/// pub struct TimeoutLayer(Duration);
///
/// pub struct Expired;
///
/// impl<T> Timeout<T> {
///     pub fn new(inner: T, timeout: Duration) -> Timeout<T> {
///         Timeout {
///             inner,
///             timeout
///         }
///     }
/// }
///
/// impl<T, Request> Service<Request> for Timeout<T>
/// where
///     T: Service<Request>,
///     T::Future: 'static,
///     T::Error: From<Expired> + 'static,
///     T::Response: 'static
/// {
///     type Response = T::Response;
///     type Error = T::Error;
///     type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///        self.inner.poll_ready().map_err(Into::into)
///     }
///
///     fn call(&mut self, req: Request) -> Self::Future {
///         let timeout = tokio_timer::sleep(self.timeout)
///             .then(|_| Err(Self::Error::from(Expired)));
///
///         let f = self.inner.call(req).select(timeout)
///             .map(|(v, _)| v)
///             .map_err(|(e, _)| e);
///
///         Box::new(f)
///     }
/// }
///
/// impl TimeoutLayer {
///     pub fn new(delay: Duration) -> Self {
///         TimeoutLayer(delay)
///     }
/// }
///
/// impl<S, Request> Layer<S, Request> for TimeoutLayer
/// where
///     S: Service<Request>,
/// {
///     type Response = S::Response;
///     type Error = S::Error;
///     type Service = Timeout<S>;
///
///     fn layer(&self, service: S) -> Timeout<S> {
///         Timeout::new(service, self.0)
///     }
/// }
///
/// ```
///
/// The above timeout implementation is decoupled from the underlying protocol
/// and is also decoupled from client or server concerns. In other words, the
/// same timeout middleware could be used in either a client or a server.
///
/// # Backpressure
///
/// Calling a `Service` which is at capacity (i.e., it is temporarily unable to process a
/// request) should result in an error. The caller is responsible for ensuring
/// that the service is ready to receive the request before calling it.
///
/// `Service` provides a mechanism by which the caller is able to coordinate
/// readiness. `Service::poll_ready` returns `Ready` if the service expects that
/// it is able to process a request.
pub trait Service<Request> {
    /// Responses given by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// Returns `Ready` when the service is able to process requests.
    ///
    /// If the service is at capacity, then `NotReady` is returned and the task
    /// is notified when the service becomes ready again. This function is
    /// expected to be called while on a task.
    ///
    /// If `Err` is returned, the service is no longer able to service requests
    /// and the caller should discard the service instance.
    ///
    /// Once `poll_ready` returns `Ready`, a request may be dispatched to the
    /// service using `call`. Until a request is dispatched, repeated calls to
    /// `poll_ready` must return either `Ready` or `Err`.
    fn poll_ready(&mut self) -> Poll<(), Self::Error>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call `poll_ready`.
    ///
    /// Before dispatching a request, `poll_ready` must be called and return
    /// `Ready`.
    ///
    /// # Panics
    ///
    /// Implementations are permitted to panic if `call` is invoked without
    /// obtaining `Ready` from `poll_ready`.
    fn call(&mut self, req: Request) -> Self::Future;
}

impl<'a, S, Request> Service<Request> for &'a mut S
where
    S: Service<Request> + 'a,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), S::Error> {
        (**self).poll_ready()
    }

    fn call(&mut self, request: Request) -> S::Future {
        (**self).call(request)
    }
}

impl<S, Request> Service<Request> for Box<S>
where
    S: Service<Request> + ?Sized,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), S::Error> {
        (**self).poll_ready()
    }

    fn call(&mut self, request: Request) -> S::Future {
        (**self).call(request)
    }
}
