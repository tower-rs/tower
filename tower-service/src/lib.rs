#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower/0.1.0")]

//! Definition of the core `Service` trait to Tower
//!
//! These traits provide the necessary abstractions for defining a request /
//! response clients and servers. They are simple but powerul and are the
//! used as the foundation for the rest of Tower.
//!
//! * [`Service`](trait.Service.html) is the primary trait and defines the request
//! / response exchange. See that trait for more details.
//! * [`NewService`](trait.NewService.html) is essentially a service factory. It
//! is responsible for generating `Service` values on demand.

#[macro_use]
extern crate futures;

use futures::{Future, IntoFuture, Poll};

use std::marker::PhantomData;
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
/// Take timeouts as an example:
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
/// impl<T, Request> Service<Request> for Timeout<T>
/// where
///     T: Service<Request>,
///     T::Error: From<Expired>,
/// {
///     type Response = T::Response;
///     type Error = T::Error;
///     type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;
///
///     fn poll_ready(&mut self) -> Poll<(), Self::Error> {
///         Ok(Async::Ready(()))
///     }
///
///     fn call(&mut self, req: Request) -> Self::Future {
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
pub trait Service<Request> {

    /// Responses given by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// A future yielding the service when it is ready to accept a request.
    fn ready(self) -> Ready<Self, Request> where Self: Sized {
        Ready {
            inner: Some(self),
            _p: PhantomData,
        }
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
    ///
    /// Calling `call` without calling `poll_ready` is permitted. The
    /// implementation must be resilient to this fact.
    fn call(&mut self, req: Request) -> Self::Future;
}

/// An asynchronous function from `Request` to a `Response` that requires polling.
///
/// A service that implements this trait acts like a future, and needs to be
/// polled for the futures returned from `call` to make progress. In particular,
/// `poll_service` must be called in a similar manner as `Future::poll`; whenever
/// the task driving the `DirectService` is notified.
pub trait DirectService<Request> {
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
    /// This is a **best effort** implementation. False positives are permitted.
    /// It is permitted for the service to return `Ready` from a `poll_ready`
    /// call and the next invocation of `call` results in an error.
    ///
    /// Implementors should call `poll_service` as necessary to finish in-flight
    /// requests.
    fn poll_ready(&mut self) -> Poll<(), Self::Error>;

    /// Returns `Ready` whenever there is no more work to be done until `call`
    /// is invoked again.
    ///
    /// Note that this method may return `NotReady` even if there are no
    /// outstanding requests, if the service has to perform non-request-driven
    /// operations (e.g., heartbeats).
    fn poll_service(&mut self) -> Poll<(), Self::Error>;

    /// A method to indicate that no more requests will be sent to this service.
    ///
    /// This method is used to indicate that a service will no longer be given
    /// another request by the caller. That is, the `call` method will
    /// be called no longer (nor `poll_service`). This method is intended to
    /// model "graceful shutdown" in various protocols where the intent to shut
    /// down is followed by a little more blocking work.
    ///
    /// Callers of this function should work it it in a similar fashion to
    /// `poll_service`. Once called it may return `NotReady` which indicates
    /// that more external work needs to happen to make progress. The current
    /// task will be scheduled to receive a notification in such an event,
    /// however.
    ///
    /// Note that this function will imply `poll_service`. That is, if a
    /// service has pending request, then it'll be flushed out during a
    /// `poll_close` operation. It is not necessary to have `poll_service`
    /// return `Ready` before a `poll_close` is called. Once a `poll_close`
    /// is called, though, `poll_service` cannot be called.
    ///
    /// # Return value
    ///
    /// This function, like `poll_service`, returns a `Poll`. The value is
    /// `Ready` once the close operation has completed. At that point it should
    /// be safe to drop the service and deallocate associated resources, and all
    /// futures returned from `call` should have resolved.
    ///
    /// If the value returned is `NotReady` then the sink is not yet closed and
    /// work needs to be done to close it. The work has been scheduled and the
    /// current task will receive a notification when it's next ready to call
    /// this method again.
    ///
    /// Finally, this function may also return an error.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` if any operation along the way during
    /// the close operation fails. An error typically is fatal for a service and is
    /// unable to be recovered from, but in specific situations this may not
    /// always be true.
    ///
    /// Note that it's also typically an error to call `call` or `poll_service`
    /// after the `poll_close` function is called. This method will *initiate*
    /// a close, and continuing to send values after that (or attempt to flush)
    /// may result in strange behavior, panics, errors, etc. Once this method is
    /// called, it must be the only method called on this `DirectService`.
    ///
    /// # Panics
    ///
    /// This method may panic or cause panics if:
    ///
    /// * It is called outside the context of a future's task
    /// * It is called and then `call` or `poll_service` is called
    fn poll_close(&mut self) -> Poll<(), Self::Error>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call any of the `poll_*`
    /// methods. If the service is at capacity and the request is unable
    /// to be handled, the returned `Future` should resolve to an error.
    ///
    /// Calling `call` without calling `poll_ready` is permitted. The
    /// implementation must be resilient to this fact.
    ///
    /// Note that for the returned future to resolve, this `DirectService`
    /// must be driven through calls to `poll_service` or `poll_close`.
    fn call(&mut self, req: Request) -> Self::Future;
}

/// Future yielding a `Service` once the service is ready to process a request
///
/// `Ready` values are produced by `Service::ready`.
pub struct Ready<T, Request> {
    inner: Option<T>,
    _p: PhantomData<fn() -> Request>,
}

/// Creates new `Service` values.
///
/// Acts as a service factory. This is useful for cases where new `Service`
/// values must be produced. One case is a TCP servier listener. The listner
/// accepts new TCP streams, obtains a new `Service` value using the
/// `NewService` trait, and uses that new `Service` value to process inbound
/// requests on that new TCP stream.
pub trait NewService<Request> {
    /// Responses given by the service
    type Response;

    /// Errors produced by the service
    type Error;

    /// The `Service` value created by this factory
    type Service: Service<Request, Response = Self::Response, Error = Self::Error>;

    /// Errors produced while building a service.
    type InitError;

    /// The future of the `Service` instance.
    type Future: Future<Item = Self::Service, Error = Self::InitError>;

    /// Create and return a new service value asynchronously.
    fn new_service(&self) -> Self::Future;
}

impl<T, Request> Future for Ready<T, Request>
where T: Service<Request>,
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

impl<F, R, E, S, Request> NewService<Request> for F
    where F: Fn() -> R,
          R: IntoFuture<Item = S, Error = E>,
          S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S;
    type InitError = E;
    type Future = R::Future;

    fn new_service(&self) -> Self::Future {
        (*self)().into_future()
    }
}

impl<S, Request> NewService<Request> for Arc<S>
where
    S: NewService<Request> + ?Sized,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S::Service;
    type InitError = S::InitError;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<S, Request> NewService<Request> for Rc<S>
where
    S: NewService<Request> + ?Sized,
{
    type Response = S::Response;
    type Error = S::Error;
    type Service = S::Service;
    type InitError = S::InitError;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<'a, S, Request> Service<Request> for &'a mut S
where
    S: Service<Request> + 'a
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
