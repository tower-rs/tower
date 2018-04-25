//! Definition of the core `Service` trait to Tower

#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower/0.1")]

#[macro_use]
extern crate futures;

use futures::{Future, IntoFuture, Poll};

use std::rc::Rc;
use std::sync::Arc;

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
/// impl Service for HelloWorld {
///     type Request = http::Request;
///     type Response = http::Response;
///     type Error = http::Error;
///     type Future = Box<Future<Item = Self::Response, Error = http::Error>>;
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
///         futures::finished(resp).boxed()
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
/// # Middleware
///
/// More often than not, all the pieces needed for writing robust, scalable
/// network applications are the same no matter the underlying protocol. By
/// unifying the API for both clients and servers in a protocol agnostic way,
/// it is possible to write middleware that provide these pieces in a
/// reusable way.
///
/// For example, take timeouts as an example:
///
/// ```rust,ignore
/// use tower_service::Service;
/// use futures::Future;
/// use std::time::Duration;
///
/// use tokio_timer::Timer;
///
/// pub struct Timeout<T> {
///     inner: T,
///     delay: Duration,
///     timer: Timer,
/// }
///
/// pub struct Expired;
///
/// impl<T> Timeout<T> {
///     pub fn new(inner: T, delay: Duration) -> Timeout<T> {
///         Timeout {
///             inner: inner,
///             delay: delay,
///             timer: Timer::default(),
///         }
///     }
/// }
///
/// impl<T> Service for Timeout<T>
///     where T: Service,
///           T::Error: From<Expired>,
/// {
///     type Request = T::Request;
///     type Response = T::Response;
///     type Error = T::Error;
///     type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///         Ok(Async::Ready(()))
///     }
///
///     fn call(&mut self, req: Self::Req) -> Self::Future {
///         let timeout = self.timer.sleep(self.delay)
///             .and_then(|_| Err(Self::Error::from(Expired)));
///
///         self.inner.call(req)
///             .select(timeout)
///             .map(|(v, _)| v)
///             .map_err(|(e, _)| e)
///             .boxed()
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
/// Calling an at capacity `Service` (i.e., it temporarily unable to process a
/// request) should result in an error. The caller is responsible for ensuring
/// that the service is ready to receive the request before calling it.
///
/// `Service` provides a mechanism by which the caller is able to coordinate
/// readiness. `Service::poll_ready` returns `Ready` if the service expects that
/// it is able to process a request.
pub trait Service {

    /// Requests handled by the service.
    type Request;

    /// Responses given by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// A future yielding the service when it is ready to accept a request.
    fn ready(self) -> Ready<Self> where Self: Sized {
        Ready { inner: Some(self) }
    }

    /// Returns `Ready` when the service is able to process requests.
    ///
    /// If the service is at capacity, then `NotReady` is returned and the task
    /// is notified when the service becomes ready again. This function is
    /// expected to be called while on a task.
    ///
    /// This is a **best effort** implementation. False positives are permitted.
    /// It is permitted for the service to return `Ready` from a `poll_ready`
    /// call and the next invocation of `call` results in an error.
    fn poll_ready(&mut self) -> Poll<(), Self::Error>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call `poll_ready`. If the
    /// service is at capacity and the request is unable to be handled, the
    /// returned `Future` should resolve to an error.
    fn call(&mut self, req: Self::Request) -> Self::Future;
}

/// An asynchronous function from `Request` to a `Response` that is always ready
/// to process a request.
///
/// `ReadyService` is similar to `Service`, except that it is always able to
/// accept a request. This request may either complete successfully or resolve
/// to an error, i.e., `ReadyService` implementations may handle out of capacity
/// situations by returning a response future that immediately resolves to an
/// error.
///
/// The `Service` trait should be prefered over this one. `ReadyService` should
/// only be used in situations where there is no way to handle back pressure.
/// When usin a `ReadyService` implementation, back pressure needs to be handled
/// via some other strategy, such as limiting the total number of in flight
/// requests.
///
/// One situation in which there is no way to handle back pressure is routing.
/// A router service receives inbound requests and dispatches them to one of N
/// inner services. In this case, one of the inner services may become "not
/// ready" while the others remain ready. It would not be ideal for the router
/// service to flag itself as "not ready" when only one of the inner services is
/// not ready, but there is no way for the router to communicate to the caller
/// which requests will be accepted and which will be rejected. The router
/// service will implement `ReadyService`, indicating to the user that they are
/// responsible for handling back pressure via some other strategy.
pub trait ReadyService {
    /// Requests handled by the service.
    type Request;

    /// Responses returned by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// Process the request and return the response asynchronously.
    fn call(&mut self, req: Self::Request) -> Self::Future;
}

/// Future yielding a `Service` once the service is ready to process a request
pub struct Ready<T> {
    inner: Option<T>,
}

/// Creates new `Service` values.
pub trait NewService {
    /// Requests handled by the service
    type Request;

    /// Responses given by the service
    type Response;

    /// Errors produced by the service
    type Error;

    /// The `Service` value created by this factory
    type Service: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    /// Errors produced while building a service.
    type InitError;

    /// The future of the `Service` instance.
    type Future: Future<Item = Self::Service, Error = Self::InitError>;

    /// Create and return a new service value asynchronously.
    fn new_service(&self) -> Self::Future;
}

impl<T> Future for Ready<T>
where T: Service,
{
    type Item = T;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<T, T::Error> {
        match self.inner {
            Some(ref mut service) => {
                let _ = try_ready!(service.poll_ready());
            }
            None => panic!("called `poll` after future completed"),
        }

        Ok(self.inner.take().unwrap().into())
    }
}

impl<F, R, E, S> NewService for F
    where F: Fn() -> R,
          R: IntoFuture<Item = S, Error = E>,
          S: Service,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;
    type InitError = E;
    type Future = R::Future;

    fn new_service(&self) -> Self::Future {
        (*self)().into_future()
    }
}

impl<S: NewService + ?Sized> NewService for Arc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S::Service;
    type InitError = S::InitError;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<S: NewService + ?Sized> NewService for Rc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Service = S::Service;
    type InitError = S::InitError;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<'a, S: Service + 'a> Service for &'a mut S {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), S::Error> {
        (**self).poll_ready()
    }

    fn call(&mut self, request: S::Request) -> S::Future {
        (**self).call(request)
    }
}

impl<S: Service + ?Sized> Service for Box<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self) -> Poll<(), S::Error> {
        (**self).poll_ready()
    }

    fn call(&mut self, request: S::Request) -> S::Future {
        (**self).call(request)
    }
}
