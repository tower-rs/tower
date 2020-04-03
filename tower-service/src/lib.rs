#![doc(html_root_url = "https://docs.rs/tower-service/0.3.0")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! Definition of the core `Service` trait to Tower
//!
//! The [`Service`] trait provides the necessary abstractions for defining
//! request / response clients and servers. It is simple but powerful and is
//! used as the foundation for the rest of Tower.

use std::future::Future;
use std::task::{Context, Poll};

/// An asynchronous function from a `Request` to a `Response`.
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
/// At a high level, the `Service::call` function represents an RPC request. The
/// `Service` value can be a server or a client.
///
/// # Server
///
/// An RPC server *implements* the `Service` trait. Requests received by the
/// server over the network are deserialized and then passed as an argument to the
/// server value. The returned response is sent back over the network.
///
/// As an example, here is how an HTTP request is processed by a server:
///
/// ```rust
/// # use std::pin::Pin;
/// # use std::task::{Poll, Context};
/// # use std::future::Future;
/// # use tower_service::Service;
///
/// use http::{Request, Response, StatusCode};
///
/// struct HelloWorld;
///
/// impl Service<Request<Vec<u8>>> for HelloWorld {
///     type Response = Response<Vec<u8>>;
///     type Error = http::Error;
///     type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;
///
///     fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
///         Poll::Ready(Ok(()))
///     }
///
///     fn call(&mut self, req: Request<Vec<u8>>) -> Self::Future {
///         // create the body
///         let body: Vec<u8> = "hello, world!\n"
///             .as_bytes()
///             .to_owned();
///         // Create the HTTP response
///         let resp = Response::builder()
///             .status(StatusCode::OK)
///             .body(body)
///             .expect("Unable to create `http::Response`");
///         
///         // create a response in a future.
///         let fut = async {
///             Ok(resp)
///         };
///
///         // Return the response as an immediate future
///         Box::pin(fut)
///     }
///
///     fn disarm(&mut self) {}
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
/// let resp = client.call(Cmd::set("foo", "this is the value of foo")).await?;
///
/// // Wait for the future to resolve
/// println!("Redis response: {:?}", resp);
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
/// use futures::FutureExt;
/// use std::future::Future;
/// use std::task::{Context, Poll};
/// use std::time::Duration;
/// use std::pin::Pin;
///
///
/// pub struct Timeout<T> {
///     inner: T,
///     timeout: Duration,
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
///     type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;
///
///     fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
///        self.inner.poll_ready(cx).map_err(Into::into)
///     }
///
///     fn call(&mut self, req: Request) -> Self::Future {
///         let timeout = tokio_timer::delay_for(self.timeout)
///             .map(|_| Err(Self::Error::from(Expired)));
///
///         let fut = Box::pin(self.inner.call(req));
///         let f = futures::select(fut, timeout)
///             .map(|either| either.factor_first().0);
///
///         Box::pin(f)
///     }
/// }
///
/// impl TimeoutLayer {
///     pub fn new(delay: Duration) -> Self {
///         TimeoutLayer(delay)
///     }
/// }
///
/// impl<S> Layer<S> for TimeoutLayer
/// {
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
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    /// Returns `Poll::Ready(Ok(()))` when the service is able to process requests.
    ///
    /// If the service is at capacity, then `Poll::Pending` is returned and the task
    /// is notified when the service becomes ready again. This function is
    /// expected to be called while on a task. Generally, this can be done with
    /// a simple `futures::future::poll_fn` call.
    ///
    /// If `Poll::Ready(Err(_))` is returned, the service is no longer able to service requests
    /// and the caller should discard the service instance.
    ///
    /// Once `poll_ready` returns `Poll::Ready(Ok(()))`, a request may be dispatched to the
    /// service using `call`. Until a request is dispatched, repeated calls to
    /// `poll_ready` must return either `Poll::Ready(Ok(()))` or `Poll::Ready(Err(_))`.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    /// Process the request and return the response asynchronously.
    ///
    /// This function is expected to be callable off task. As such,
    /// implementations should take care to not call `poll_ready`.
    ///
    /// Before dispatching a request, `poll_ready` must be called and return
    /// `Poll::Ready(Ok(()))`.
    ///
    /// # Panics
    ///
    /// Implementations are permitted to panic if `call` is invoked without
    /// obtaining `Poll::Ready(Ok(()))` from `poll_ready`.
    fn call(&mut self, req: Request) -> Self::Future;

    /// Undo a successful call to `poll_ready`.
    ///
    /// Once a call to `poll_ready` returns `Poll::Ready(Ok(()))`, the service must keep capacity
    /// set aside for the coming request. `disarm` allows you to give up that reserved capacity if
    /// you decide you do not wish to issue a request after all. After calling `disarm`, you must
    /// call `poll_ready` until it returns `Poll::Ready(Ok(()))` before attempting to issue another
    /// request.
    ///
    /// Returns `false` if capacity has not been reserved for this service (usually because
    /// `poll_ready` was not previously called, or did not succeed).
    ///
    /// In general, when implementing this method for anything that wraps a service, you will
    /// simply want to forward the call to `disarm` to the `disarm` method of the inner service.
    ///
    /// # Motivation
    ///
    /// If `poll_ready` reserves part of a service's finite capacity, callers need to send an item
    /// shortly after `poll_ready` succeeds. If they do not, an idle caller may take up all the
    /// slots of the channel, and prevent active callers from getting any requests through.
    /// Consider this code that forwards from a channel to a `BufferService` (which has limited
    /// capacity):
    ///
    /// ```rust,ignore
    /// let mut fut = None;
    /// loop {
    ///   if let Some(ref mut fut) = fut {
    ///     let _ = ready!(fut.poll(cx));
    ///     let _ = fut.take();
    ///   }
    ///   ready!(buffer.poll_ready(cx));
    ///   if let Some(item) = ready!(rx.poll_next(cx)) {
    ///     fut = Some(buffer.call(item));
    ///   } else {
    ///     break;
    ///   }
    /// }
    /// ```
    ///
    /// If many such forwarders exist, and they all forward into a single (cloned) `BufferService`,
    /// then any number of the forwarders may be waiting for `rx.next` at the same time. While they
    /// do, they are effectively each reducing the buffer's capacity by 1. If enough of these
    /// forwarders are idle, forwarders whose `rx` _do_ have elements will be unable to find a spot
    /// for them through `poll_ready`, and the system will deadlock.
    ///
    /// `disarm` solves this problem by allowing you to give up the reserved slot if you find that
    /// you will not be calling `call` for the foreseeable future. We can then fix the code above
    /// by writing:
    ///
    /// ```rust,ignore
    /// let mut fut = None;
    /// loop {
    ///   if let Some(ref mut fut) = fut {
    ///     let _ = ready!(fut.poll(cx));
    ///     let _ = fut.take();
    ///   }
    ///   ready!(buffer.poll_ready(cx));
    ///   let item = rx.poll_next(cx);
    ///   if let Poll::Ready(Ok(_)) = item {
    ///     // we're going to send the item below, so don't disarm
    ///   } else {
    ///     // give up our slot, since we won't need it for a while
    ///     buffer.disarm();
    ///   }
    ///   if let Some(item) = ready!(item) {
    ///     fut = Some(buffer.call(item));
    ///   } else {
    ///     break;
    ///   }
    /// }
    /// ```
    ///
    /// Note that if we later decide that we _do_ want to call `Service::call`, then we first call
    /// `Service::poll_ready` again, since the call to `disarm` made the service not ready.
    ///
    /// # Panics
    ///
    /// Implementations are permitted to panic if `disarm` is invoked without
    /// obtaining `Poll::Ready(Ok(()))` from `poll_ready`.
    fn disarm(&mut self);
}

impl<'a, S, Request> Service<Request> for &'a mut S
where
    S: Service<Request> + 'a,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        (**self).poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> S::Future {
        (**self).call(request)
    }

    fn disarm(&mut self) {
        (**self).disarm()
    }
}

impl<S, Request> Service<Request> for Box<S>
where
    S: Service<Request> + ?Sized,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        (**self).poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> S::Future {
        (**self).call(request)
    }

    fn disarm(&mut self) {
        (**self).disarm()
    }
}
