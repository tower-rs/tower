use Service;

/// Middleware, a function from one `Service` to another.
///
/// `Middleware` provides a common abstraction for processes that can be
/// applied to services to construct new services. More often than not, many of
/// the pieces needed to write robust, scalable network services can be
/// modularized and reused.
///
/// Middleware can be applied directly to a service with `wrap`, and two
/// compatible middleware can be composed together into a new middleware with
/// `chain`.
///
/// # Example
///
/// ```rust,ignore
/// use std::time::Duration;
///
/// use futures::Future;
/// use tokio_timer::Timer;
///
/// use tower::{Service, Middleware};
///
/// pub struct Timeout {
///     pub delay: Duration,
///     pub timer: Timer,
/// }
///
/// pub struct Expired;
///
/// impl<S> Middleware<S> for Timeout where
///     S: Service,
///     S::Error: From<Expired>,
/// {
///     type Wrapped = TimeoutService<S>;
///     fn wrap(self, upstream: S) -> TimeoutService<S> {
///         TimeoutService { upstream, timeout: self }
///     }
/// }
///
/// pub struct TimeoutService<S> {
///     timeout: Timeout,
///     upstream: S,
/// }
///
/// impl<S> Service for TimeoutService<S> where
///     S: Service,
///     S::Error: From<Expired>,
/// {
///     type Request = S::Request;
///     type Response = S::Response;
///     type Error = S::Error;
///     type Future = BoxFuture<Self::Response, Self::Error>>;
///     fn call(&self, req: Self::Request) -> Self::Future {
///         let timeout = self.timeout.timer.sleep(self.timeout.delay)
///                           .and_then(|_| Err(S::Error::from(Expired)));
///
///         self.upstream.call(req).select(timeout)
///             .map(|(v, _)| v)
///             .map_err(|(e, _)| e)
///             .boxed()
/// }
/// ```
///
/// The above timeout implementation is decoupled from the underlying protocol
/// and is also decoupled from client or server concerns. In other words, the
/// same timeout middleware could be used in either a client or a server.
pub trait Middleware<S: Service> {
    /// The service produced by applying this Middleware to S.
    type Wrapped: Service;

    /// Wrap a Service with this Middleware.
    fn wrap(self, upstream: S) -> Self::Wrapped;

    /// Chain this middleware together with another, producing a combined
    /// middleware.
    fn chain<M: Middleware<Self::Wrapped>>(self, middleware: M) -> MiddlewareChain<Self, M>
        where Self: Sized
    {
        MiddlewareChain {
            inner: self,
            outer: middleware,
        }
    }
}

/// A chained pair of middleware, which itself also implements Middleware.
pub struct MiddlewareChain<M1, M2> {
    inner: M1,
    outer: M2,
}

impl<S, M1, M2> Middleware<S> for MiddlewareChain<M1, M2>
where
    S: Service,
    M1: Middleware<S>,
    M2: Middleware<M1::Wrapped>,
{
    type Wrapped = M2::Wrapped;

    fn wrap(self, upstream: S) -> Self::Wrapped {
        upstream.wrap(self.inner).wrap(self.outer)
    }
}
