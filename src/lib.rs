//! Definition of the core `Service` trait to Tower
//!
//! More information can be found on [the trait] itself and online at
//! [https://tower.rs](https://tower.rs)
//!
//! [the trait]: trait.Service.html

#![deny(missing_docs)]
#![doc(html_root_url = "https://docs.rs/tower/0.1")]

extern crate futures;

mod middleware;

pub use middleware::{Middleware, MiddlewareChain};

use futures::{Future, IntoFuture};

use std::io;
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
///     fn call(&self, req: http::Request) -> Self::Future {
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
pub trait Service {

    /// Requests handled by the service.
    type Request;

    /// Responses given by the service.
    type Response;

    /// Errors produced by the service.
    type Error;

    /// The future response value.
    type Future: Future<Item = Self::Response, Error = Self::Error>;

    /// Process the request and return the response asynchronously.
    fn call(&self, req: Self::Request) -> Self::Future;

    /// Wrap this service in middleware.
    fn wrap<M: Middleware<Self>>(self, middleware: M) -> M::Wrapped
        where Self: Sized
    {
        middleware.wrap(self)
    }
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
    type Instance: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    /// The future of the `Service` instance.
    type Future: Future<Item = Self::Instance, Error = io::Error>;

    /// Create and return a new service value asynchronously.
    fn new_service(&self) -> Self::Future;
}

impl<F, R, S> NewService for F
    where F: Fn() -> R,
          R: IntoFuture<Item = S, Error = io::Error>,
          S: Service,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Instance = S;
    type Future = R::Future;

    fn new_service(&self) -> Self::Future {
        (*self)().into_future()
    }
}

impl<S: NewService + ?Sized> NewService for Arc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Instance = S::Instance;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<S: NewService + ?Sized> NewService for Rc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Instance = S::Instance;
    type Future = S::Future;

    fn new_service(&self) -> Self::Future {
        (**self).new_service()
    }
}

impl<S: Service + ?Sized> Service for Box<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, request: S::Request) -> S::Future {
        (**self).call(request)
    }
}

impl<S: Service + ?Sized> Service for Rc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, request: S::Request) -> S::Future {
        (**self).call(request)
    }
}

impl<S: Service + ?Sized> Service for Arc<S> {
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, request: S::Request) -> S::Future {
        (**self).call(request)
    }
}
